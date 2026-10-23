//! The §12.1 driver: trace, then every (point, fault) and every observation.
use super::oracles::Oracle;
use super::scenarios::Fake;
use super::world::{self, World};
use super::*;
use crate::fault::{self, Fault, Point};
use crate::gate::{Gate, GateError, GateState, Wait};
use crate::id::ResourceId;
use crate::root::StateRoot;
use crate::run;
use crate::store::Store;
use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, Stdio};

const SCENARIO: &str = "EFFIGY_KERNEL_FAULT_SCENARIO";
const CHILD: &str = "faults::driver::fault_child";
/// Every kernel wait in a scenario is bounded well inside this.
const CHILD_BUDGET: Duration = Duration::from_secs(60);

/// One §12.1 scenario expressed with kernel primitives and strict fakes.
pub(super) trait Scenario: Sync {
    fn name(&self) -> &'static str;
    /// Every oracle here must judge at least one subject across the drive.
    fn oracles(&self) -> &'static [Oracle];
    /// Points the scenario must cross, so a refactor cannot silently drop one.
    fn required(&self) -> &'static [Point];
    /// Points whose crossing count legitimately varies run to run.
    fn variable(&self) -> &'static [Point] {
        &[]
    }
    /// Observability points: faults there must not change the outcome (O9).
    fn observability(&self) -> &'static [Point] {
        &[]
    }
    /// The resources whose gates a stalled holder must keep exclusive.
    fn owners(&self) -> Vec<ResourceId>;
    fn prepare(&self, _home: &Path) {}
    /// Process-wide warm-up before arming (a probe guard's cold start), so
    /// crossing counts match the trace in every process.
    fn warm(&self) {}
    /// The faulted operation; returns the outcome's exit code when it settles.
    fn run(&self, home: &Path) -> Option<i32>;
    /// Scenario facts for the oracles (foreign fixtures, replace targets).
    fn facts(&self, _home: &Path, _world: &mut World) {}
    /// Extra checks while a child is stopped at a point (Stall).
    fn stalled(&self, _home: &Path, _pid: u32) {}
}

#[derive(Debug, Default)]
struct Tally {
    timing: BTreeMap<String, Duration>,
    judged: BTreeMap<Oracle, usize>,
    runs: usize,
    delivered: BTreeSet<(Point, Fault)>,
}
impl Tally {
    fn encode(&self) -> String {
        let mut out = format!("runs {}\n", self.runs);
        for (oracle, judged) in &self.judged {
            out += &format!("judged {oracle:?} {judged}\n");
        }
        for (point, fault) in &self.delivered {
            out += &format!("delivered {point:?} {fault:?}\n");
        }
        for (phase, spent) in &self.timing {
            out += &format!("timing {phase} {}\n", spent.as_nanos());
        }
        out
    }
    fn merge(&mut self, encoded: &str) {
        for line in encoded.lines() {
            let fields: Vec<&str> = line.split(' ').collect();
            match fields.as_slice() {
                ["runs", n] => self.runs += n.parse::<usize>().expect("runs"),
                ["judged", oracle, n] => {
                    let oracle = *Oracle::ALL
                        .iter()
                        .find(|o| format!("{o:?}") == *oracle)
                        .expect("oracle");
                    *self.judged.entry(oracle).or_default() += n.parse::<usize>().expect("judged");
                }
                ["delivered", point, fault] => {
                    let point = *Point::ALL
                        .iter()
                        .find(|p| format!("{p:?}") == *point)
                        .expect("point");
                    let fault = *Fault::ALL
                        .iter()
                        .find(|f| format!("{f:?}") == *fault)
                        .expect("fault");
                    self.delivered.insert((point, fault));
                }
                ["timing", phase, nanos] => {
                    *self.timing.entry((*phase).into()).or_default() +=
                        Duration::from_nanos(nanos.parse().expect("nanos"));
                }
                _ => panic!("bad shard tally line {line}"),
            }
        }
    }
}
/// One faulted run: a fault at the `nth` crossing of a point, or `Unknown`
/// at one observation.
#[derive(Debug, Clone, Copy)]
enum Job {
    Point(Point, usize, Fault),
    Observation(usize),
}
fn jobs(trace: &[Point], observations: usize) -> Vec<Job> {
    let mut nth = BTreeMap::<Point, usize>::new();
    let mut jobs = vec![];
    for point in trace {
        let n = *nth.entry(*point).and_modify(|n| *n += 1).or_insert(0);
        jobs.extend(Fault::ALL.iter().map(|fault| Job::Point(*point, n, *fault)));
    }
    jobs.extend((0..observations).map(Job::Observation));
    jobs
}
/// Worker processes per drive. Shards are processes, never threads: the probe
/// runtime, the foreign fixture and child reaping are process-wide.
const SHARDS: usize = 6;
const PLAN: &str = "EFFIGY_KERNEL_FAULT_PLAN";
const SHARD: &str = "EFFIGY_KERNEL_FAULT_SHARD";

/// Traces `scenario`, then injects every fault at every crossing and
/// `Unknown` at every observation, recovering and checking each time.
pub(super) fn drive(scenario: &dyn Scenario) {
    let started = Instant::now();
    let (trace, observations, baseline) = {
        let _foreign = ForeignGuard;
        traced(scenario)
    };
    for point in scenario.required() {
        assert!(
            trace.contains(point),
            "{}: trace misses {point:?}: {trace:?}",
            scenario.name()
        );
    }
    let plan = tempfile::Builder::new()
        .prefix("kf-plan-")
        .tempdir()
        .expect("plan dir");
    let encoded: Vec<String> = trace.iter().map(|p| format!("{p:?}")).collect();
    std::fs::write(
        plan.path().join("plan"),
        format!(
            "{}\n{}\n{}",
            observations,
            baseline.map_or("none".into(), |b| b.to_string()),
            encoded.join(" ")
        ),
    )
    .expect("plan");
    let shards: Vec<_> = (0..SHARDS)
        .map(|shard| {
            let child = Command::new(std::env::current_exe().expect("test binary"))
                .args([
                    SHARD_CHILD,
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(SCENARIO, scenario.name())
                .env(PLAN, plan.path())
                .env(SHARD, format!("{shard}/{SHARDS}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .expect("shard");
            (shard, child)
        })
        .collect();
    let mut tally = Tally::default();
    let mut failed = Vec::new();
    for (shard, mut child) in shards {
        let status = child.wait().expect("shard wait");
        if !status.success() {
            failed.push((shard, status));
            continue;
        }
        let encoded = std::fs::read_to_string(plan.path().join(format!("tally-{shard}")))
            .expect("shard tally");
        tally.merge(&encoded);
    }
    assert!(
        failed.is_empty(),
        "{}: failed shards: {failed:?}",
        scenario.name()
    );
    for point in &trace {
        if scenario.variable().contains(point) {
            continue;
        }
        for fault in Fault::ALL {
            assert!(
                tally.delivered.contains(&(*point, fault)),
                "{}: {point:?} {fault:?} never delivered",
                scenario.name()
            );
        }
    }
    for oracle in scenario.oracles() {
        assert!(
            tally.judged.get(oracle).copied().unwrap_or(0) > 0,
            "{}: {oracle:?} judged nothing across {} runs",
            scenario.name(),
            tally.runs
        );
    }
    eprintln!(
        "fault-suite {}: {} points, {} observations, {} faulted runs, {:?} wall, {SHARDS} shards ({:?})",
        scenario.name(),
        trace.len(),
        observations,
        tally.runs,
        started.elapsed(),
        tally.timing
    );
}
/// The foreign fixture ends with its process's drive work, even on panic.
struct ForeignGuard;
impl Drop for ForeignGuard {
    fn drop(&mut self) {
        super::scenarios::stop_foreign();
    }
}
const SHARD_CHILD: &str = "faults::driver::fault_shard";
#[test]
#[ignore = "fault shard process"]
fn fault_shard() {
    let (Ok(name), Some(plan), Ok(shard)) = (
        std::env::var(SCENARIO),
        std::env::var_os(PLAN),
        std::env::var(SHARD),
    ) else {
        return;
    };
    let plan = PathBuf::from(plan);
    let scenario = super::scenarios::by_name(&name);
    let text = std::fs::read_to_string(plan.join("plan")).expect("plan");
    let mut lines = text.lines();
    let observations: usize = lines
        .next()
        .expect("observations")
        .parse()
        .expect("observations");
    let baseline = lines.next().expect("baseline").parse::<i32>().ok();
    let trace: Vec<Point> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(|name| {
            *Point::ALL
                .iter()
                .find(|p| format!("{p:?}") == name)
                .expect("point")
        })
        .collect();
    let (index, count) = shard.split_once('/').expect("shard spec");
    let (index, count): (usize, usize) = (
        index.parse().expect("shard"),
        count.parse().expect("shards"),
    );
    let _foreign = ForeignGuard;
    let mut tally = Tally::default();
    for (_, job) in jobs(&trace, observations)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % count == index)
    {
        run_job(scenario, job, baseline, &mut tally);
    }
    std::fs::write(plan.join(format!("tally-{index}")), tally.encode()).expect("tally");
}
fn run_job(scenario: &dyn Scenario, job: Job, baseline: Option<i32>, tally: &mut Tally) {
    let label = match job {
        Job::Point(point, n, fault) => format!("{}: {point:?}#{n} {fault:?}", scenario.name()),
        Job::Observation(index) => format!("{}: observation {index} Unknown", scenario.name()),
    };
    let home = home();
    fault::set_helper_log(Some(helper_log(&home.home())));
    scenario.prepare(&home.home());
    let started = Instant::now();
    let (fired, exit, phase) = match job {
        Job::Point(point, n, fault @ (Fault::Kill | Fault::Stall)) => (
            child(scenario, &home.home(), point, n, fault),
            None,
            "child",
        ),
        Job::Point(point, n, fault) => {
            scenario.warm();
            fault::arm_nth(point, n, fault);
            let exit = scenario.run(&home.home());
            (fault::fired(), exit, "in-process")
        }
        Job::Observation(index) => {
            scenario.warm();
            fault::arm_observation(index);
            let exit = scenario.run(&home.home());
            (fault::fired(), exit, "observation")
        }
    };
    *tally.timing.entry(phase.into()).or_default() += started.elapsed();
    if !fired {
        match job {
            Job::Point(point, ..) => assert!(
                scenario.variable().contains(&point),
                "{label}: fault was not delivered at a deterministic point; crossed {:?}",
                fault::trace()
            ),
            // Observation counts vary only where polling loops observe.
            Job::Observation(_) => {}
        }
        fault::arm(None);
        reap_exited();
        return;
    }
    if let Job::Point(point, _, fault) = job {
        tally.delivered.insert((point, fault));
    }
    let events = fault::events();
    fault::arm(None);
    let started = Instant::now();
    let mut world = recover(scenario, &home.home(), &label);
    *tally.timing.entry("recover".into()).or_default() += started.elapsed();
    world.events.splice(0..0, events);
    // A failed log or status write, or a jumped wall clock, is an
    // observability fault; injected Unknown is an observation fault.
    if let Job::Point(point, _, Fault::Enospc | Fault::Eperm | Fault::ClockJump) = job {
        if scenario.observability().contains(&point) {
            world.outcomes.push((
                label.clone(),
                baseline.expect("baseline exit"),
                exit.unwrap_or(-1),
            ));
        }
    }
    judge(&world, &label, tally);
    reap_exited();
}
fn traced(scenario: &dyn Scenario) -> (Vec<Point>, usize, Option<i32>) {
    let home = home();
    fault::set_helper_log(Some(helper_log(&home.home())));
    scenario.prepare(&home.home());
    scenario.warm();
    fault::arm(None);
    let exit = scenario.run(&home.home());
    let trace = fault::trace();
    let observations = fault::observations().len();
    let events = fault::events();
    fault::arm(None);
    // The unfaulted run is itself checked: a green trace is the baseline.
    let mut world = recover(scenario, &home.home(), "trace");
    world.events.splice(0..0, events);
    let mut tally = Tally::default();
    judge(&world, &format!("{}: trace", scenario.name()), &mut tally);
    reap_exited();
    (trace, observations, exit)
}
fn judge(world: &World, label: &str, tally: &mut Tally) {
    tally.runs += 1;
    for oracle in Oracle::ALL {
        match oracle.check(world) {
            Ok(judged) => *tally.judged.entry(oracle).or_default() += judged,
            Err(failure) => panic!("{label}: {failure}"),
        }
    }
}

fn helper_log(home: &Path) -> PathBuf {
    aux(home).join("helpers")
}
/// Waits until every helper spawned for this HOME, registered or not, and
/// every recorded sentinel has exited: helpers are legitimate writers.
/// Anchors are nonwriters and must survive until recovery has stopped them.
fn quiesce(home: &Path, label: &str, after_recovery: bool) {
    wait_until(
        &format!("{label}: helpers exit"),
        Duration::from_secs(40),
        || {
            let logged = std::fs::read_to_string(helper_log(home)).unwrap_or_default();
            let helpers: Vec<_> = logged
                .lines()
                .filter_map(|line| {
                    if let Some(line) = line.strip_prefix("anchor ") {
                        after_recovery.then(|| world::identity(line)).flatten()
                    } else {
                        world::identity(line)
                    }
                })
                .collect();
            fault::uninjected(|| {
                World::capture_runs(home)
                    .iter()
                    .filter_map(|run| run.sentinel.clone())
                    .chain(helpers)
                    .all(|helper| !world::identity_alive(&helper))
            })
        },
    );
}
/// Status reads (O10), the generic settle of every unresolved run, the
/// scenario's own recovery, then the captured world.
fn recover(scenario: &dyn Scenario, home: &Path, label: &str) -> World {
    quiesce(home, label, false);
    let before = world::tree(home);
    observe_everything(home);
    let after = world::tree(home);
    let status_diff = world::diff(&before, &after);
    let cx = mutate();
    let mut fake = Fake::new(home);
    if let Ok(root) = StateRoot::effects(&cx, home) {
        for run in World::capture_runs(home) {
            let resolved = run.phase == "closed"
                && (run.cleanup.as_deref() == Some("complete") || run.acknowledged);
            if resolved {
                continue;
            }
            let owner = scenario
                .owners()
                .into_iter()
                .chain(super::scenarios::known_owners())
                .find(|owner| owner.name().to_string() == run.owner)
                .unwrap_or_else(|| panic!("{label}: unknown owner {}", run.owner));
            let result = run::settle_interrupted_wait(
                &cx,
                &root,
                (&owner, run.id),
                next(),
                &recovery_cleanup(&cx),
                &mut fake,
                Wait::Until(Instant::now() + Duration::from_secs(20)),
            );
            if let Err(error) = result {
                panic!("{label}: recovery settle of {} failed: {error:?}", run.id);
            }
        }
    }
    // Rerun: every owner admits a fresh successor after recovery, unless its
    // residue is unacknowledged, which must refuse one (O1b, K5).
    let runs = World::capture_runs(home);
    let mut blocked = BTreeMap::new();
    for (n, owner) in (250u8..).zip(scenario.owners().iter()) {
        let name = owner.name().to_string();
        let residue: Vec<String> = runs
            .iter()
            .filter(|r| {
                r.owner == name
                    && r.phase == "closed"
                    && r.cleanup.as_deref() != Some("complete")
                    && !r.acknowledged
            })
            .map(|r| r.id.to_string())
            .collect();
        let refused = match super::scenarios::rerun(home, owner, n) {
            Ok(()) => false,
            Err(run::RunError::PredecessorUnsettled { .. }) if !residue.is_empty() => true,
            Err(error) => panic!(
                "{label}: rerun of {} refused ({error:?}) beside {residue:?}",
                owner.label()
            ),
        };
        for run in residue {
            blocked.insert(run, refused);
        }
    }
    quiesce(home, label, true);
    let mut world = World::capture(home);
    world.successor_blocked = blocked;
    world.diffs.push((format!("{label} status"), status_diff));
    if let Ok(root) = StateRoot::effects(&cx, home) {
        if let Ok(store) = Store::open(&cx, &root) {
            world.integrity = Some(store.check_integrity().map_err(|e| format!("{e:?}")));
        }
    }
    scenario.facts(home, &mut world);
    world
}
/// Every Observe-class read the kernel offers over this HOME.
fn observe_everything(home: &Path) {
    let cx = observe();
    let Observed::Verified(root, _) = StateRoot::observe(&cx, home) else {
        return;
    };
    let Observed::Verified(reader, _) = Store::read(&cx, &root) else {
        return;
    };
    for run in World::capture_runs(home) {
        let _ = run::status(&cx, &root, &reader, run.id, next());
        let _ = run::metadata(&cx, &root, run.id);
    }
    for owner in super::scenarios::known_owners() {
        let _ = Gate::probe(&cx, &root, &owner);
        if owner.kind() == crate::id::ResourceKind::Env {
            let _ = run::Attachments::holders(&cx, &root, &reader, &owner);
        }
    }
}

/// Runs the scenario in a re-executed child armed with `Kill` or `Stall`.
fn child(scenario: &dyn Scenario, home: &Path, point: Point, n: usize, fault: Fault) -> bool {
    let child = Command::new(std::env::current_exe().expect("test binary"))
        .args([
            CHILD,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(SCENARIO, scenario.name())
        .env("HOME", home)
        .env("EFFIGY_KERNEL_TEST_FAULT", fault::env_spec(point, n, fault))
        .env("EFFIGY_KERNEL_TEST_HELPERS", helper_log(home))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("fault child");
    let pid = child.id();
    // The child is reaped here with WUNTRACED; std's handle is not used again.
    drop(child);
    let deadline = Instant::now() + CHILD_BUDGET;
    let mut stalled = false;
    loop {
        let mut status = 0;
        // SAFETY: our own child; WNOHANG never blocks.
        let reaped =
            unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG | libc::WUNTRACED) };
        assert!(reaped >= 0, "waitpid: {}", std::io::Error::last_os_error());
        if reaped == pid as i32 && libc::WIFSTOPPED(status) {
            assert_eq!(fault, Fault::Stall, "{point:?}: unexpected stop");
            stalled = true;
            inspect_stalled(scenario, home, pid, point);
            // SAFETY: continuing our own stopped child.
            unsafe { libc::kill(pid as i32, libc::SIGCONT) };
            continue;
        }
        if reaped == pid as i32 {
            if libc::WIFSIGNALED(status) {
                assert_eq!(
                    libc::WTERMSIG(status),
                    libc::SIGKILL,
                    "{point:?}: child died"
                );
                assert_eq!(fault, Fault::Kill, "{point:?}: child killed unarmed");
                return true;
            }
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "{}: {point:?}#{n} {fault:?} child failed: {status:#x}",
                scenario.name()
            );
            return stalled;
        }
        if Instant::now() >= deadline {
            // SAFETY: our own child, recorded pid, never a pattern kill.
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            panic!(
                "{}: {point:?}#{n} {fault:?} child exceeded bound",
                scenario.name()
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
/// A stopped holder is live: its gates stay held and exclusive (K2, O4).
fn inspect_stalled(scenario: &dyn Scenario, home: &Path, pid: u32, point: Point) {
    let mut world = World::default();
    world.live_holders.insert(pid);
    let cx = observe();
    if let Observed::Verified(root, _) = StateRoot::observe(&cx, home) {
        for owner in scenario.owners() {
            if matches!(
                Gate::probe(&cx, &root, &owner),
                Observed::Verified(GateState::Held(_), _)
            ) {
                let effects = mutate();
                let root = StateRoot::effects(&effects, home).expect("root");
                match Gate::acquire(
                    &effects,
                    &root,
                    std::slice::from_ref(&owner),
                    Wait::FailFast,
                ) {
                    Err(GateError::Busy { .. }) => {}
                    other => panic!(
                        "{}: stalled holder at {point:?} lost {}: {other:?}",
                        scenario.name(),
                        owner.label()
                    ),
                }
            }
        }
    }
    let captured = World::capture(home);
    world.gates = captured.gates;
    world.scopes_held = captured.scopes_held;
    if let Err(failure) = Oracle::O4.check(&world) {
        panic!("{}: stalled at {point:?}: {failure}", scenario.name());
    }
    scenario.stalled(home, pid);
}

#[test]
#[ignore = "fault child process"]
fn fault_child() {
    let Ok(name) = std::env::var(SCENARIO) else {
        return;
    };
    let home = PathBuf::from(std::env::var_os("HOME").expect("fault home"));
    let scenario = super::scenarios::by_name(&name);
    scenario.warm();
    fault::arm_from_env();
    scenario.run(&home);
    fault::arm(None);
}
