//! §12.1 scenarios the kernel layer expresses with strict fakes. Engine
//! scenarios are placeholders at the end, owned by their engine lanes.
use super::driver::{drive, Scenario};
use super::oracles::Oracle;
use super::world::{self, install_audits, World};
use super::*;
use crate::custody::{Custody, Lifetime, OwnedProcess, ProcessReceipt, ProcessSpec, Terminal};
use crate::fault::Point;
use crate::gate::{Gate, Wait};
use crate::id::*;
use crate::ident::ProcessIdentity;
use crate::observe::{AbsenceProof, UnknownReason};
use crate::outcome::{RequiredObligation, WorkloadOutcome};
use crate::root::StateRoot;
use crate::run::*;
use crate::store::Store;
use crate::supervise::{LifecyclePlan, Placement as Supervision};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::process::CommandExt;

// --- Identities. ---

fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16])
}
pub(super) fn task(n: u8) -> ResourceId {
    TaskId::new(
        checkout(),
        "",
        Surface::Published,
        &format!("task{n}"),
        None,
    )
    .expect("task id")
    .resource()
}
pub(super) fn env() -> ResourceId {
    ResourceId::environment(checkout(), "stack").expect("environment id")
}
fn vault() -> ResourceId {
    ResourceId::store(Scope::Machine, "vault").expect("vault id")
}
pub(super) fn id(n: u8) -> RunId {
    RunId::new(1, [n; 10]).expect("run id")
}
/// Every owner a scenario uses, so recovery can name a recorded owner.
pub(super) fn known_owners() -> Vec<ResourceId> {
    let mut owners: Vec<ResourceId> = (0..=12).map(task).collect();
    owners.extend([env(), vault()]);
    owners
}
pub(super) fn invocation(owner: ResourceId, gates: Vec<ResourceId>) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout(), "", Surface::Published, "task", None).expect("task"),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates,
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: crate::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("invocation")
}
pub(super) fn begin<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    owner: ResourceId,
    n: u8,
    custody: Option<Supervision>,
    acknowledge: Vec<RunId>,
    fake: &mut impl SettlementAdapter,
) -> Result<Run<'a>, RunError> {
    Run::begin(
        cx,
        root,
        &invocation(owner, vec![]),
        BeginOpts {
            id: id(n),
            wait: Wait::FailFast,
            acknowledge,
            next: next(),
            custody: custody.map(|placement| LifecyclePlan {
                executable: fixture(),
                placement,
            }),
        },
        &cleanup(cx),
        fake,
    )
}
/// The recovery step's rerun: a fresh successor of `owner`, settled at once.
/// The caller distinguishes unacknowledged residue (K5) from other errors.
/// The settle rides the caller's recovery budget: nothing is injected at this
/// step, so inheriting the one-second injected-scenario bound turned plain
/// store latency under shard load into a spurious `Interrupted(TimedOut)`
/// refusal (effigy#618).
pub(super) fn rerun(
    home: &Path,
    owner: &ResourceId,
    n: u8,
    cleanup: &CleanupContext,
) -> Result<(), RunError> {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let run = begin(&cx, &root, owner.clone(), n, None, vec![], &mut fake)?;
    run.settle(cleanup, &mut fake).map(drop)
}

// --- The durable strict fake: adapter effects, environment backend calls. ---

/// A durable strict fake. Objects are files, so a SIGKILL cannot erase
/// evidence; a second application or undo panics; unmodelled backend
/// operations panic. An effect keyed `stuck` refuses undo until an
/// operator marker `unstick` exists.
#[derive(Debug)]
pub(super) struct Fake {
    home: PathBuf,
}
impl Fake {
    pub(super) fn new(home: &Path) -> Self {
        Self { home: home.into() }
    }
    fn object(&self, key: &str) -> PathBuf {
        assert!(!key.contains('/'), "fake key");
        self.home.join(format!("fake-{key}"))
    }
    fn log(&self, line: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.join("calls"))
            .expect("call log");
        writeln!(file, "{line}").expect("call log");
        file.sync_all().expect("call log");
    }
    fn apply(&self, key: &str, fence: &Fence) {
        assert!(
            !crate::store::in_transaction(),
            "adapter call inside a transaction"
        );
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.object(key))
            .unwrap_or_else(|e| panic!("fake {key} applied twice: {e}"));
        file.write_all(&serde_json::to_vec(fence).expect("fence"))
            .expect("fake object");
        file.sync_all().expect("fake object");
        self.log(&format!("apply {key}"));
    }
    /// A backend call, logged with the holders the store shows at that instant.
    pub(super) fn backend(&self, op: &str, fence: &Fence) {
        assert!(
            ["up", "down", "handover"].contains(&op),
            "unmodelled backend operation {op}"
        );
        // An instrument, not an observation: a raw read-only connection that
        // fault injection cannot reach.
        let holders = rusqlite::Connection::open_with_flags(
            self.home.join(".effigy/state/kernel.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .and_then(|conn| {
            conn.query_row(
                "SELECT count(*) FROM attachments WHERE environment=?1 AND state<>'released'",
                [fence.resource.name().to_string()],
                |r| r.get::<_, i64>(0),
            )
        })
        .unwrap_or(-1);
        self.log(&format!(
            "env:{op} {}@{} holders={holders}",
            fence.resource.label(),
            fence.generation.get()
        ));
    }
}
impl SettlementAdapter for Fake {
    fn observe(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Fence> {
        assert!(!crate::store::in_transaction());
        let key = effect.spec.payload.as_str().expect("fake payload key");
        match std::fs::read(self.object(key)) {
            Ok(bytes) => Observed::Verified(
                serde_json::from_slice(&bytes).expect("fake fence"),
                crate::observe::Evidence::new("fake object".into(), None).expect("evidence"),
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Observed::Absent(AbsenceProof::NoFile { name: key.into() })
            }
            Err(e) => panic!("unmodelled fake error: {e}"),
        }
    }
    fn release(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<()> {
        assert!(!crate::store::in_transaction());
        let key = effect.spec.payload.as_str().expect("fake payload key");
        if key == "stuck" && !self.home.join("unstick").exists() {
            return Observed::Unknown(UnknownReason::Inconsistent("stuck fake".into()));
        }
        std::fs::remove_file(self.object(key)).expect("exactly one undo");
        self.log(&format!("release {key}"));
        Observed::Verified(
            (),
            crate::observe::Evidence::new("fake undo".into(), None).expect("evidence"),
        )
    }
    fn reconcile(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<Reconciled> {
        self.release(effect, cx).map(|()| Reconciled::Released)
    }
}
fn spec(target: Fence, key: &str, settlement: Settlement) -> EffectSpec {
    EffectSpec {
        target,
        kind: "fake".into(),
        payload: Value::String(key.into()),
        settlement,
        required: true,
    }
}
/// Intent, external effect, callback, natural completion: the engine shape.
pub(super) fn apply(run: &mut Run<'_>, fake: &Fake, key: &str) -> Result<(), RunError> {
    let ticket = run.intend(spec(
        run.fence().clone(),
        key,
        Settlement::Release(Value::Null),
    ))?;
    fake.apply(key, run.fence());
    run.applied(&ticket, Value::Bool(true))?;
    run.completed(&ticket, RequiredObligation::Applied)
}
pub(super) fn settle(run: Run<'_>, cx: &EffectContext, fake: &mut Fake) -> Option<i32> {
    run.settle(&cleanup(cx), fake).ok().map(|o| o.exit_code())
}
/// On any refusal the engine settles; a settle failure is left for recovery.
macro_rules! attempt {
    ($run:ident, $cx:expr, $fake:expr, $e:expr) => {
        match $e {
            Ok(value) => value,
            Err(_) => return settle($run, $cx, $fake).and(None),
        }
    };
}

// --- Fixtures: workloads and foreign sentinels. ---

/// A workload that announces its pid and writes one line of output. `LINGER`
/// keeps it alive as the fixture, whose custody nonce the oracles can read.
fn workload(home: &Path, body: &str, lifetime: Lifetime) -> ProcessSpec {
    ProcessSpec {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            format!(
                "echo $$ >> '{}'; echo out; {body}",
                home.join("pids").display()
            )
            .into(),
        ],
        cwd: home.into(),
        env: BTreeMap::from([(
            "EFFIGY_FAULT_FIXTURE".into(),
            crate::custody::EnvValue::Plain(fixture_path().into()),
        )]),
        grace: Duration::from_millis(50),
        lifetime,
        terminal: Terminal::None,
        label: "fault-workload".into(),
    }
}
const LINGER: &str = "exec \"$EFFIGY_FAULT_FIXTURE\" nonce-workload";
fn finite() -> Lifetime {
    Lifetime::Finite(Duration::from_secs(20))
}
/// One foreign process per test binary, in its own group, that records every
/// catchable signal. It must stay alive and unsignalled through every fault
/// and recovery of the whole drive (O12).
struct Foreign {
    dir: tempfile::TempDir,
    child: std::sync::Mutex<std::process::Child>,
}
static FOREIGN: std::sync::OnceLock<Foreign> = std::sync::OnceLock::new();
fn foreign_fixture() -> &'static Foreign {
    FOREIGN.get_or_init(|| {
        let dir = tempfile::Builder::new()
            .prefix("kf-foreign-")
            .tempdir()
            .expect("foreign dir");
        let signals = dir.path().join("signals");
        let traps: String = ["TERM", "INT", "HUP", "CONT", "QUIT", "USR1", "USR2"]
            .iter()
            .map(|sig| format!("trap 'echo {sig} >> \"{}\"' {sig}; ", signals.display()))
            .collect();
        let child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("{traps}while :; do /bin/sleep 0.05; done"))
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("foreign fixture");
        Foreign {
            dir,
            child: std::sync::Mutex::new(child),
        }
    })
}
fn foreign_pid() -> u32 {
    foreign_fixture().child.lock().expect("foreign").id()
}
/// Reads the foreign fixture into the world: alive, unstopped, unsignalled.
fn foreign_facts(world: &mut World) {
    let fixture = foreign_fixture();
    let pid = foreign_pid();
    // A trap runs once its 50 ms sleep returns.
    std::thread::sleep(Duration::from_millis(60));
    let process = world::proc(pid, b"EFFIGY_CUSTODY_NONCE=");
    let mut signals: Vec<String> = std::fs::read_to_string(fixture.dir.path().join("signals"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    if matches!(
        crate::ident::process_state(pid),
        Observed::Verified(crate::ident::ProcessState::Stopped, _)
    ) {
        signals.push("STOP".into());
    }
    world.foreign.push((process, signals));
}
/// Ends the drive's foreign fixture: our own child and its own group.
pub(super) fn stop_foreign() {
    let Some(fixture) = FOREIGN.get() else {
        return;
    };
    let mut child = fixture.child.lock().expect("foreign");
    // SAFETY: our own recorded child's own process group; never a pattern kill.
    unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
    let _ = child.wait();
}

// --- Anchor retirement pin. ---

/// Freezes each launch's recorded anchor against its own self-retirement for
/// the length of the faulted run. The `AnchorStop`/`AnchorStopped` points
/// observe settlement's retirement of a *verified live* anchor; since
/// effigy#520 an anchor also retires itself within one poll of its group
/// closing — legitimately, before settlement ever reaches it — so a
/// free-running anchor made those required points a coin flip under shard
/// load (effigy#618). A stopped anchor is still positive evidence
/// (`ProcessState::Stopped` verifies), settlement still stops it last by
/// identity, and recovery stops it the same way, so the pin holds exactly the
/// lifecycle boundary the points observe instead of racing it. No authority is
/// manufactured: the identity, nonce and group evidence are the launch's own.
/// Dropping the pin continues the anchors: on the settled path they are
/// already gone, and on an abandoned run they retire themselves as the
/// contract allows, never outliving their group.
#[derive(Default)]
pub(super) struct AnchorPin {
    pids: Vec<u32>,
}
impl AnchorPin {
    pub(super) fn pin(&mut self, processes: &[OwnedProcess]) {
        for anchor in processes.iter().filter_map(|process| process.anchor()) {
            // The generation check narrows, but cannot close, the race
            // between anchor retirement and signalling a reused pid. macOS
            // has no atomic signal-by-identity; see member_signal.
            if !world::identity_alive(&anchor) {
                continue;
            }
            // SAFETY: the recorded anchor of this scenario's own launch, its
            // generation held immediately above; a retirement plus reuse
            // between the two calls is not excluded.
            if unsafe { libc::kill(anchor.pid as i32, libc::SIGSTOP) } == 0 {
                self.pids.push(anchor.pid);
            }
        }
    }
}
impl Drop for AnchorPin {
    fn drop(&mut self) {
        for pid in &self.pids {
            // SAFETY: recorded anchors of this scenario's own launches.
            unsafe { libc::kill(*pid as i32, libc::SIGCONT) };
        }
    }
}

// --- Custody scenarios: plain task, sequence, multi-child test. ---

#[derive(Debug)]
struct Steps {
    name: &'static str,
    owner: u8,
    /// Sequential steps; each inner list is spawned together.
    steps: &'static [&'static [&'static str]],
}
const CUSTODY_POINTS: &[Point] = &[
    Point::BeginBefore,
    Point::GateAcquire,
    Point::BeginGated,
    Point::BeginRecovered,
    Point::BeginAdmitted,
    Point::LifecycleBefore,
    Point::LifecycleCommit,
    Point::LifecycleAfter,
    Point::IntendBefore,
    Point::IntendAfter,
    Point::AnchorSpawn,
    Point::AnchorRecord,
    Point::AnchorStop,
    Point::AnchorStopped,
    Point::SpawnFork,
    Point::SpawnBeforeRecord,
    Point::AppliedBefore,
    Point::AppliedAfter,
    Point::SpawnBeforeGo,
    Point::SpawnAfterGo,
    Point::PhaseBefore,
    Point::PhaseAfter,
    Point::SettleBefore,
    Point::SettleStopping,
    Point::SettleEffectBefore,
    Point::SettleEffectAfter,
    Point::SettleCloseBefore,
    Point::SettleCloseAfter,
    Point::SentinelClose,
    Point::StoreCommitBefore,
    Point::StoreCommitAfter,
];
impl Scenario for Steps {
    fn name(&self) -> &'static str {
        self.name
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O1a,
            Oracle::O2,
            Oracle::O3,
            Oracle::O4,
            Oracle::O6,
            Oracle::O9,
            Oracle::O10,
            Oracle::O12,
            Oracle::O13,
        ]
    }
    fn required(&self) -> &'static [Point] {
        CUSTODY_POINTS
    }
    fn variable(&self) -> &'static [Point] {
        &[Point::CaptureWrite]
    }
    fn observability(&self) -> &'static [Point] {
        &[Point::CaptureWrite]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(self.owner)]
    }
    fn prepare(&self, home: &Path) {
        install_audits(home);
        foreign_pid();
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).ok()?;
        let fake = &mut Fake::new(home);
        let mut run = begin(
            &cx,
            &root,
            task(self.owner),
            self.owner,
            Some(Supervision::Inline),
            vec![],
            fake,
        )
        .ok()?;
        let mut failed = None;
        let mut anchors = AnchorPin::default();
        for step in self.steps {
            // Keep each group alive during its anchor pin, then run the
            // original workload. Fast exits otherwise race anchor retirement.
            let specs = step
                .iter()
                .map(|body| {
                    let mut spec = workload(home, &format!("read -r _; {body}"), finite());
                    spec.terminal = Terminal::Pipe;
                    spec
                })
                .collect();
            let mut processes = attempt!(run, &cx, fake, Custody::spawn_all(&mut run, specs, &cx));
            anchors.pin(&processes);
            for process in &processes {
                attempt!(
                    run,
                    &cx,
                    fake,
                    process.write_stdin(b"\n", Instant::now() + Duration::from_secs(1))
                );
            }
            // The last member of a group may linger; settle stops it.
            let waited = processes.len() - usize::from(processes.len() > 1);
            for process in processes.iter_mut().take(waited) {
                // Workloads exit in milliseconds; an Unknown exit waits out
                // only this bound, never the whole operation deadline.
                let exit = match process.wait(&cx.operation().child(Duration::from_secs(1))) {
                    Observed::Verified(exit, _) => exit,
                    _ => return settle(run, &cx, fake).and(None),
                };
                attempt!(run, &cx, fake, process.complete(&mut run, exit));
                if exit.code != 0 {
                    failed = Some(exit.code);
                }
            }
        }
        let workload = match failed {
            Some(code) => WorkloadOutcome::Failed { code },
            None => WorkloadOutcome::Succeeded,
        };
        attempt!(run, &cx, fake, run.finish(workload));
        settle(run, &cx, fake)
    }
    fn facts(&self, _home: &Path, world: &mut World) {
        foreign_facts(world);
    }
}
static STEPS: [Steps; 3] = [
    Steps {
        name: "plain-task",
        owner: 1,
        steps: &[&["exit 0"]],
    },
    Steps {
        name: "sequence",
        owner: 2,
        steps: &[&["exit 0"], &["exit 0"]],
    },
    Steps {
        name: "multi-child",
        owner: 3,
        steps: &[&[
            "exit 0",
            "exit 3",
            "\"$EFFIGY_FAULT_FIXTURE\" nonce-workload & echo $! >> pids; wait",
        ]],
    },
];
#[test]
fn faults_plain_task_every_point_and_observation() {
    drive(by_name("plain-task"));
}
#[test]
fn faults_sequence_every_point_and_observation() {
    drive(by_name("sequence"));
}
#[test]
fn faults_multi_child_test_every_point_and_observation() {
    drive(by_name("multi-child"));
}

// --- Detached managed run with an environment attachment. ---

/// Publishes the environment `active` at generation 1, as its engine would.
pub(super) fn active_environment(home: &Path) {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let mut run = begin(&cx, &root, env(), 200, None, vec![], &mut fake).expect("env run");
    Attachments::publish_active(&mut run).expect("active");
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
}
pub(super) fn env_fence(generation: u64) -> Fence {
    Fence {
        resource: env(),
        generation: Generation::new(generation),
    }
}
#[derive(Debug)]
struct ManagedAttach;
impl Scenario for ManagedAttach {
    fn name(&self) -> &'static str {
        "managed-attach"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O1a,
            Oracle::O2,
            Oracle::O3,
            Oracle::O4,
            Oracle::O5,
            Oracle::O6,
            Oracle::O10,
            Oracle::O12,
            Oracle::O13,
        ]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::AttachBefore,
            Point::AttachAfter,
            Point::RequestBefore,
            Point::RequestAfter,
            Point::AttachmentReleaseBefore,
            Point::AttachmentReleaseAfter,
            Point::SpawnBeforeGo,
            Point::LifecycleCommit,
            Point::SettleCloseAfter,
        ]
    }
    fn variable(&self) -> &'static [Point] {
        &[Point::CaptureWrite]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(4), env()]
    }
    fn prepare(&self, home: &Path) {
        install_audits(home);
        active_environment(home);
        foreign_pid();
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).ok()?;
        let fake = &mut Fake::new(home);
        let mut run = begin(
            &cx,
            &root,
            task(4),
            4,
            Some(Supervision::Detached),
            vec![],
            fake,
        )
        .ok()?;
        attempt!(
            run,
            &cx,
            fake,
            Attachments::attach(&mut run, &env_fence(1), AttachMode::Borrow, Wait::FailFast)
        );
        let spec = workload(home, LINGER, Lifetime::Open(OpenReason::DevServer));
        let mut anchors = AnchorPin::default();
        let _process = attempt!(run, &cx, fake, Custody::spawn(&mut run, spec, &cx));
        anchors.pin(std::slice::from_ref(&_process));
        attempt!(run, &cx, fake, run.advance(Phase::Active));
        // A front end's stop is a durable fenced request, never a signal.
        let stop = Request {
            id: id(220),
            run: run.id(),
            expected: run.fence().clone(),
            action: RequestAction::Stop,
        };
        attempt!(run, &cx, fake, request(&cx, &root, &stop));
        let pending = attempt!(run, &cx, fake, run.requests());
        for request in &pending {
            attempt!(run, &cx, fake, run.resolve_request(request));
        }
        attempt!(
            run,
            &cx,
            fake,
            run.interrupt(
                &cleanup(&cx),
                WorkloadOutcome::Cancelled {
                    cause: StopCause::Requested,
                    signal: None,
                },
            )
        );
        settle(run, &cx, fake)
    }
    fn facts(&self, _home: &Path, world: &mut World) {
        foreign_facts(world);
    }
}
#[test]
fn faults_detached_managed_run_with_environment_attachment() {
    drive(&ManagedAttach);
}

// --- Environment up, down and handover with a concurrent attach. ---

#[derive(Debug)]
struct Environment;
/// One environment transition under its own gate: permit, intent, backend.
pub(super) fn transition(
    home: &Path,
    n: u8,
    op: &str,
    settlement: Settlement,
    publish: bool,
) -> Result<Option<Fence>, ()> {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).map_err(drop)?;
    let fake = &mut Fake::new(home);
    let mut run = begin(&cx, &root, env(), n, None, vec![], fake).map_err(drop)?;
    let fence = run.fence().clone();
    let result = (|| {
        let mut permit = Attachments::permit(&mut run).map_err(drop)?;
        let key = format!("{op}-{}", fence.generation.get());
        let ticket = permit
            .intend(EffectSpec {
                target: fence.clone(),
                kind: "backend".into(),
                payload: Value::String(key),
                settlement,
                required: true,
            })
            .map_err(drop)?;
        fake.backend(op, &fence);
        permit.applied(&ticket, Value::Bool(true)).map_err(drop)?;
        permit
            .completed(&ticket, RequiredObligation::Applied)
            .map_err(drop)?;
        if publish {
            Attachments::publish_active(&mut run).map_err(drop)?;
        }
        run.finish(WorkloadOutcome::Succeeded).map_err(drop)
    })();
    let settled = run.settle(&cleanup(&cx), fake);
    match (result, settled) {
        (Ok(()), Ok(_)) => Ok(Some(fence)),
        // A transition refused while held is the correct outcome (R4).
        _ => Ok(None),
    }
}
/// A holder attaches; while it holds, a transition must be refused.
fn held_transition(home: &Path, holder: u8, at: &Fence, refused: u8, op: &str) -> Option<()> {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).ok()?;
    let fake = &mut Fake::new(home);
    let mut run = begin(&cx, &root, task(holder), holder, None, vec![], fake).ok()?;
    attempt!(
        run,
        &cx,
        fake,
        Attachments::attach(&mut run, at, AttachMode::Borrow, Wait::FailFast)
    );
    attempt!(
        run,
        &cx,
        fake,
        apply(&mut run, fake, &format!("holder-{holder}"))
    );
    // The concurrent transition: its permit sees the attachment.
    let _ = transition(home, refused, op, Settlement::Retain, false);
    attempt!(run, &cx, fake, run.finish(WorkloadOutcome::Succeeded));
    run.settle(&cleanup(&cx), fake).ok().map(drop)
}
impl Scenario for Environment {
    fn name(&self) -> &'static str {
        "environment"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O2,
            Oracle::O3,
            Oracle::O4,
            Oracle::O5,
            Oracle::O6,
            Oracle::O8,
            Oracle::O10,
            Oracle::O11,
            Oracle::O13,
        ]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::AttachBefore,
            Point::AttachAfter,
            Point::AttachmentReleaseBefore,
            Point::AttachmentReleaseAfter,
            Point::IntendBefore,
            Point::AppliedAfter,
            Point::AdapterBefore,
            Point::AdapterAfter,
            Point::SettleEffectAfter,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![env(), task(5), task(6)]
    }
    fn prepare(&self, home: &Path) {
        install_audits(home);
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let up = transition(home, 10, "up", Settlement::Retain, true).ok()??;
        held_transition(home, 5, &up, 11, "down")?;
        transition(home, 12, "down", Settlement::Irreversible, false).ok()??;
        let again = transition(home, 13, "up", Settlement::Retain, true).ok()??;
        held_transition(home, 6, &again, 14, "handover")?;
        transition(home, 15, "handover", Settlement::Retain, true).ok()??;
        // A stale generation is refused (K7), whatever else was faulted.
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).ok()?;
        let stale = Request {
            id: id(221),
            run: id(13),
            expected: again,
            action: RequestAction::Stop,
        };
        assert!(
            request(&cx, &root, &stale).is_err(),
            "stale request accepted"
        );
        Some(0)
    }
}
#[test]
fn faults_environment_up_down_handover_with_concurrent_attach_part_0() {
    super::driver::drive_part(&Environment, 0, 3);
}
#[test]
fn faults_environment_up_down_handover_with_concurrent_attach_part_1() {
    super::driver::drive_part(&Environment, 1, 3);
}
#[test]
fn faults_environment_up_down_handover_with_concurrent_attach_part_2() {
    super::driver::drive_part(&Environment, 2, 3);
}
// Keep every arm selected by the environment selector present at compile time.
const _: [fn(); 3] = [
    faults_environment_up_down_handover_with_concurrent_attach_part_0,
    faults_environment_up_down_handover_with_concurrent_attach_part_1,
    faults_environment_up_down_handover_with_concurrent_attach_part_2,
];

// --- A Closed predecessor with residue, followed by begin. ---

#[derive(Debug)]
struct Residue;
impl Scenario for Residue {
    fn name(&self) -> &'static str {
        "residue-predecessor"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O1b,
            Oracle::O2,
            Oracle::O3,
            Oracle::O4,
            Oracle::O5,
            Oracle::O6,
            Oracle::O8,
            Oracle::O10,
            Oracle::O13,
        ]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::BeginRecovered,
            Point::SettleEffectBefore,
            Point::AdapterBefore,
            Point::BeginAdmitted,
            Point::FailedBefore,
            Point::FailedAfter,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(7)]
    }
    fn prepare(&self, home: &Path) {
        install_audits(home);
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).expect("root");
        let mut fake = Fake::new(home);
        let mut run = begin(&cx, &root, task(7), 30, None, vec![], &mut fake).expect("begin");
        apply(&mut run, &fake, "stuck").expect("apply");
        run.finish(WorkloadOutcome::Succeeded).expect("finish");
        let wait_out = cx.operation().cleanup(Duration::from_secs(1)).unwrap();
        let outcome = run.settle(&wait_out, &mut fake).expect("settle");
        assert_eq!(
            outcome.cleanup(),
            crate::outcome::CleanupOutcome::Incomplete
        );
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).ok()?;
        let fake = &mut Fake::new(home);
        // Without acknowledgement the residue blocks admission (K5); O13
        // judges any admission that slipped through.
        if let Ok(run) = begin(&cx, &root, task(7), 31, None, vec![], fake) {
            let _ = run.settle(&cleanup(&cx), fake);
        }
        let mut run = begin(&cx, &root, task(7), 32, None, vec![id(30)], fake).ok()?;
        // An attempt the adapter proves never happened is Failed, not undone.
        let skipped = attempt!(
            run,
            &cx,
            fake,
            run.intend(spec(
                run.fence().clone(),
                "skipped",
                Settlement::Release(Value::Null)
            ))
        );
        attempt!(
            run,
            &cx,
            fake,
            run.failed(
                &skipped,
                AbsenceProof::NoFile {
                    name: "skipped".into()
                }
            )
        );
        attempt!(run, &cx, fake, apply(&mut run, fake, "work"));
        attempt!(run, &cx, fake, run.finish(WorkloadOutcome::Succeeded));
        settle(run, &cx, fake)
    }
}
#[test]
fn faults_closed_predecessor_with_residue_then_begin() {
    drive(&Residue);
}

// --- Owner loss, then recovery stop over a reused and a foreign group. ---

#[derive(Debug)]
struct OwnerLoss;
impl Scenario for OwnerLoss {
    fn name(&self) -> &'static str {
        "owner-loss"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O1b,
            Oracle::O8,
            Oracle::O2,
            Oracle::O3,
            Oracle::O4,
            Oracle::O6,
            Oracle::O10,
            Oracle::O12,
        ]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::GateAcquire,
            Point::SettleStopping,
            Point::SettleEffectBefore,
            Point::SettleEffectAfter,
            Point::SettleCloseBefore,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(8)]
    }
    /// The lost owner journalled two process effects: its real workload, in
    /// its own group with the custody nonce, and a receipt whose pid now
    /// names a foreign process with a different start identity (reuse).
    fn prepare(&self, home: &Path) {
        install_audits(home);
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).expect("root");
        let mut fake = Fake::new(home);
        let mut run = begin(&cx, &root, task(8), 40, None, vec![], &mut fake).expect("begin");
        let reused = foreign_pid();
        let fence = run.fence().clone();
        let process_spec = || EffectSpec {
            target: fence.clone(),
            kind: crate::custody::KIND.into(),
            payload: serde_json::json!({"label": "lost"}),
            settlement: Settlement::Release(Value::Null),
            required: true,
        };
        let spec = process_spec();
        let orphan_ticket = run.intend_process(spec).expect("intent");
        let orphan = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "echo $$ >> '{}'; exec '{}' nonce-workload",
                home.join("pids").display(),
                fixture_path().display()
            ))
            .env(
                "EFFIGY_CUSTODY_NONCE",
                format!("{}:{}", id(40), orphan_ticket.sequence()),
            )
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("orphan");
        wait_until("orphan announced", Duration::from_secs(5), || {
            !world::pids(&home.join("pids")).is_empty()
        });
        let identity = verified(crate::ident::process_identity(orphan.id()));
        run.applied(
            &orphan_ticket,
            serde_json::to_value(ProcessReceipt {
                identity: identity.clone(),
                pgid: orphan.id(),
                nonce: format!("{}:{}", id(40), orphan_ticket.sequence()),
                grace_millis: 20,
            })
            .expect("receipt"),
        )
        .expect("applied");
        let spec = process_spec();
        let reused_ticket = run.intend_process(spec).expect("intent");
        let mut stale = verified(crate::ident::process_identity(reused));
        stale.start = identity.start;
        run.applied(
            &reused_ticket,
            serde_json::to_value(ProcessReceipt {
                identity: stale,
                pgid: reused,
                nonce: format!("{}:{}", id(40), reused_ticket.sequence()),
                grace_millis: 20,
            })
            .expect("receipt"),
        )
        .expect("applied");
        std::mem::forget(orphan);
        // Owner loss: liveness ends, cleanup does not happen.
        drop(run);
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).ok()?;
        settle_interrupted(
            &cx,
            &root,
            &task(8),
            id(40),
            next(),
            &cleanup(&cx),
            &mut Fake::new(home),
        )
        .ok()
        .map(|o| o.exit_code())
    }
    fn facts(&self, _home: &Path, world: &mut World) {
        foreign_facts(world);
    }
}
#[test]
fn faults_owner_loss_then_recovery_stop_over_reused_and_foreign_groups() {
    drive(&OwnerLoss);
}

// --- Observe with guarded subprocess and HTTP probes. ---

fn probe_runtime() -> std::sync::Arc<crate::probe::ProbeRuntime> {
    static RUNTIME: std::sync::OnceLock<std::sync::Arc<crate::probe::ProbeRuntime>> =
        std::sync::OnceLock::new();
    RUNTIME
        .get_or_init(|| crate::probe::ProbeRuntime::install(fixture()).expect("runtime"))
        .clone()
}
/// Long enough that fork, identity and registration finish under host load;
/// the probe itself never completes, so only the deadline ends it.
const PROBE_DEADLINE: Duration = Duration::from_millis(1500);
/// How long the sampler follows a probe's exit after `deadline + grace`.
///
/// The guard's wake-up is the host scheduler's to grant, so the sampler waits
/// for the exit event instead of judging one instant (effigy#955). Measured on
/// an 18-core host at load 14-63, a probe was confirmed gone between 0.2 ms and
/// 887 ms after that bound (median 75 ms, p90 553 ms over 167 observations;
/// those are upper bounds on the departure, because a later observation cannot
/// show an earlier exit). 2 s is ~2.25x the worst observation, so a delayed but
/// healthy guard passes, while a guard that acts 3 s late — or never — leaves a
/// live probe for O14 to reject. This is a hung-guard ceiling, not a promptness
/// promise: nothing here asserts when the scheduler runs the guard.
const PROBE_EXIT_WAIT: Duration = Duration::from_secs(2);
fn probe_bound() -> Duration {
    PROBE_DEADLINE + crate::probe::GRACE
}
fn probe_clock() -> u128 {
    crate::ident::boot_clock()
        .verified()
        .expect("boot clock")
        .elapsed
        .as_nanos()
}
/// Samples announced probe processes after their evidence bound.
///
/// The guard's deadline, not this sampler's sleep, decides when a probe must
/// end, and the guard's own wake-up is the host scheduler's to grant. So the
/// sample waits for the exit event up to `wait`: a delayed guard is measured
/// by when the probe actually left, while a guard that never acts still leaves
/// a live probe for O14 to reject.
fn probe_survivors(home: &Path) -> Vec<world::Proc> {
    probe_survivors_within(home, PROBE_EXIT_WAIT)
}
fn probe_survivors_within(home: &Path, wait: Duration) -> Vec<world::Proc> {
    let pids = world::pids(&aux(home).join("probe-pids"));
    if pids.is_empty() {
        return vec![];
    }
    let deadline: u128 = std::fs::read_to_string(aux(home).join("probe-deadline"))
        .expect("registered probe deadline")
        .parse()
        .expect("deadline nanos");
    let bound = deadline + crate::probe::GRACE.as_nanos();
    let remaining = bound.saturating_sub(probe_clock());
    std::thread::sleep(Duration::from_nanos(
        remaining.try_into().expect("probe bound"),
    ));
    let limit = probe_clock() + wait.as_nanos();
    let live = |pid: u32| {
        !matches!(
            crate::ident::process_state(pid),
            Observed::Absent(_) | Observed::Verified(crate::ident::ProcessState::Zombie, _)
        )
    };
    while pids.iter().any(|pid| live(*pid)) && probe_clock() < limit {
        std::thread::sleep(Duration::from_micros(200));
    }
    pids.into_iter()
        .map(|pid| world::proc(pid, b"EFFIGY_PROBE_NONCE="))
        .collect()
}
/// A guarded probe the host scheduler delays past its deadline is not a
/// survivor: the sampler follows the exit event, and still reports a probe that
/// a broken guard never ends — or one it leaves past the 2 s ceiling.
#[test]
fn probe_evidence_follows_the_exit_event_not_one_sampled_instant() {
    let late = home();
    let mut delayed = std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 0.4"])
        .spawn()
        .expect("delayed probe");
    std::fs::write(
        aux(&late.home()).join("probe-pids"),
        delayed.id().to_string(),
    )
    .expect("announced probe");
    // The workload's window is already over: only the guard's reaction is
    // outstanding, exactly as when a busy host delays the guard itself.
    let past = probe_clock().saturating_sub(500_000_000);
    std::fs::write(aux(&late.home()).join("probe-deadline"), past.to_string())
        .expect("registered deadline");
    let sampled = probe_survivors(&late.home());
    delayed.wait().expect("delayed probe");
    assert!(!sampled[0].alive, "an exited probe is not a survivor");
    // A guard that never acts leaves the probe alive past the sampler's own
    // ceiling, and O14 still rejects it.
    let hung = home();
    let mut lingering = std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 10"])
        .spawn()
        .expect("lingering probe");
    std::fs::write(
        aux(&hung.home()).join("probe-pids"),
        lingering.id().to_string(),
    )
    .expect("announced probe");
    let past = probe_clock().saturating_sub(500_000_000);
    std::fs::write(aux(&hung.home()).join("probe-deadline"), past.to_string())
        .expect("registered deadline");
    let world = World {
        probes: probe_survivors_within(&hung.home(), Duration::from_millis(100)),
        probe_bound: Some(probe_bound()),
        ..World::default()
    };
    assert!(Oracle::O14.check(&world).is_err(), "a hung probe survives");
    // SAFETY: our own recorded child, killed by its pid.
    unsafe { libc::kill(lingering.id() as i32, libc::SIGKILL) };
    let _ = lingering.wait();
    // A guard that acts 3 s late is over the ceiling: the probe is still live
    // when the sampler gives up, so O14 reports the survivor. This is the
    // deliberate price of the 2 s ceiling, measured against a real process.
    let overdue = home();
    let mut lagging = std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 3"])
        .spawn()
        .expect("late probe");
    std::fs::write(
        aux(&overdue.home()).join("probe-pids"),
        lagging.id().to_string(),
    )
    .expect("announced probe");
    let grace = crate::probe::GRACE.as_nanos();
    let just_due = probe_clock().saturating_sub(grace);
    std::fs::write(
        aux(&overdue.home()).join("probe-deadline"),
        just_due.to_string(),
    )
    .expect("registered deadline");
    let world = World {
        probes: probe_survivors(&overdue.home()),
        probe_bound: Some(probe_bound()),
        ..World::default()
    };
    assert!(
        Oracle::O14.check(&world).is_err(),
        "a guard 3s late must still report a survivor"
    );
    // SAFETY: our own recorded child, killed by its pid.
    unsafe { libc::kill(lagging.id() as i32, libc::SIGKILL) };
    let _ = lagging.wait();
}
#[derive(Debug)]
struct ObserveProbes;
impl Scenario for ObserveProbes {
    fn name(&self) -> &'static str {
        "observe-probes"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O4,
            Oracle::O6,
            Oracle::O10,
            Oracle::O12,
            Oracle::O14,
        ]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::ProbeObservation,
            Point::ProbeFork,
            Point::ProbeRegister,
            Point::ProbeBeforeGo,
            Point::ProbeAfterGo,
            Point::ProbeCloseBefore,
            Point::ProbeConnect,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(9)]
    }
    /// Leaves a live, acknowledged guard: a closure exchange that missed its
    /// grace under load retires the guard, and the next probe restarts it.
    fn warm(&self) {
        use crate::probe::ProbeCommand;
        let runtime = probe_runtime();
        let cx = observe().with_probes(runtime.clone());
        for _ in 0..5 {
            let warmed = cx.probe_process(ProbeCommand {
                argv: vec!["/usr/bin/true".into()],
                cwd: "/".into(),
                env: BTreeMap::new(),
                deadline: Duration::from_secs(10),
                max_output: 64,
            });
            if matches!(warmed, Observed::Verified(..))
                && matches!(runtime.guard_identity(&cx), Observed::Verified(..))
            {
                return;
            }
        }
        panic!("probe guard never warmed");
    }
    fn prepare(&self, home: &Path) {
        install_audits(home);
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).expect("root");
        drop(Gate::acquire(&cx, &root, &[task(9)], Wait::FailFast).expect("gate inode"));
        foreign_pid();
    }
    fn run(&self, home: &Path) -> Option<i32> {
        use crate::probe::{HttpReadMethod, ObservationTarget, ProbeCommand, StatusSet};
        let cx = observe().with_probes(probe_runtime());
        let before = world::tree(home);
        if let Observed::Verified(root, _) = StateRoot::observe(&cx, home) {
            let _ = Gate::probe(&cx, &root, &task(9));
            if let Observed::Verified(reader, _) = Store::read(&cx, &root) {
                let _ = Attachments::holders(&cx, &root, &reader, &env());
            }
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
        let address = listener.local_addr().ok()?;
        listener.set_nonblocking(true).ok()?;
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.set_nonblocking(false);
                    let mut request = [0u8; 1024];
                    let _ = std::io::Read::read(&mut stream, &mut request);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                    return;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        });
        let http = cx.connect(ObservationTarget::Http {
            address,
            host: "localhost".into(),
            path: "/health".into(),
            method: HttpReadMethod::Get,
            expect: StatusSet::default(),
            connect_timeout: Duration::from_millis(500),
            total_timeout: Duration::from_secs(2),
        });
        // A connection that never happened leaves the server to its bound.
        if !matches!(http, Observed::Unknown(_)) {
            let _ = server.join();
        }
        // One guarded subprocess per run, last: under load a closure exchange
        // may retire the guard, so no later crossing may depend on it. The
        // completing probe is `warm`, before arming.
        let announce = aux(home).join("probe-pids");
        std::fs::write(aux(home).join("probe-deadline"), "").ok()?;
        let hung = cx.probe_process(ProbeCommand {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                // The fixture, not a platform binary: macOS hides a platform
                // binary's environment, and with it the probe nonce (O14).
                format!(
                    "echo $$ >> '{}'; exec '{}' nonce-workload",
                    announce.display(),
                    fixture_path().display()
                )
                .into(),
            ],
            cwd: aux(home),
            env: BTreeMap::new(),
            deadline: PROBE_DEADLINE,
            max_output: 1024,
        });
        if !crate::fault::fired()
            && crate::fault::trace().contains(&Point::ProbeFork)
            && !crate::fault::trace().contains(&Point::ProbeRegister)
        {
            eprintln!("probe registration was not reached: {hung:?}");
        }
        assert!(
            matches!(hung, Observed::Unknown(_)),
            "a hung probe must read Unknown: {hung:?}"
        );
        let after = world::tree(home);
        let diff = world::diff(&before, &after);
        std::fs::write(aux(home).join("observe-diff"), diff.join("\n")).ok()?;
        Some(0)
    }
    fn facts(&self, home: &Path, world: &mut World) {
        if let Ok(diff) = std::fs::read_to_string(aux(home).join("observe-diff")) {
            world.diffs.push((
                "observe probes".into(),
                diff.lines().map(str::to_owned).collect(),
            ));
        }
        world.probes = probe_survivors(home);
        world.probe_bound = Some(probe_bound());
        foreign_facts(world);
    }
    /// The observer is stopped; its guard must still end the probe (I-O6).
    fn stalled(&self, home: &Path, _pid: u32) {
        let mut world = World {
            probes: probe_survivors(home),
            probe_bound: Some(probe_bound()),
            ..World::default()
        };
        world.live_holders.clear();
        if let Err(failure) = Oracle::O14.check(&world) {
            panic!("observe-probes stalled observer: {failure}");
        }
    }
}
#[test]
fn faults_observe_with_guarded_subprocess_and_http_probes() {
    drive(&ObserveProbes);
}

// --- Vault write: a generation pointer and a single atomic replace. ---

#[derive(Debug)]
struct Vault;
const PARTS: [&str; 2] = ["cipher", "wrap"];
fn files(home: &Path) -> crate::fs::DirFd {
    use std::os::unix::fs::PermissionsExt;
    let dir = home.join("files");
    std::fs::create_dir_all(&dir).expect("files");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("mode");
    crate::fs::DirFd::open(&dir).expect("files")
}
fn name(text: &str) -> crate::fs::FileName {
    crate::fs::FileName::new(text).expect("file name")
}
fn vault_write(home: &Path, generation: u64) -> Result<(), crate::store::StoreError> {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home)?;
    let guard = Gate::acquire(&cx, &root, &[vault()], Wait::FailFast)?;
    let bodies: Vec<String> = PARTS.iter().map(|p| format!("{p}-{generation}")).collect();
    let stems: Vec<_> = PARTS.iter().map(|p| name(p)).collect();
    let parts: Vec<(&crate::fs::FileName, &[u8])> = stems
        .iter()
        .zip(&bodies)
        .map(|(stem, body)| (stem, body.as_bytes()))
        .collect();
    crate::record::commit_generation(
        &cx,
        &guard,
        &files(home),
        &name("local.vault"),
        Generation::new(generation),
        &parts,
        1024,
    )?;
    crate::record::atomic_replace(
        &cx,
        &guard,
        &files(home),
        &name("settings"),
        format!("settings-{generation}").as_bytes(),
        crate::record::ReplaceOpts {
            keep_previous: true,
            ..crate::record::ReplaceOpts::default()
        },
    )
    .map(drop)
}
impl Scenario for Vault {
    fn name(&self) -> &'static str {
        "vault"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[Oracle::O4, Oracle::O7, Oracle::O10]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::GateAcquire,
            Point::ReplaceOpen,
            Point::ReplaceWrite,
            Point::ReplaceFsync,
            Point::ReplaceLinkPrev,
            Point::ReplaceRename,
            Point::ReplaceDirFsync,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![vault()]
    }
    fn prepare(&self, home: &Path) {
        vault_write(home, 1).expect("generation 1");
    }
    fn run(&self, home: &Path) -> Option<i32> {
        vault_write(home, 2).ok().map(|()| 0)
    }
    fn facts(&self, home: &Path, world: &mut World) {
        let set =
            crate::record::read_generation(&observe(), &files(home), &name("local.vault"), 1024);
        let joined = |generation: u64| {
            PARTS
                .iter()
                .map(|p| format!("{p}={p}-{generation}"))
                .collect::<Vec<_>>()
                .join(",")
                .into_bytes()
        };
        let actual = set.verified().map(|set| {
            set.parts
                .iter()
                .map(|(stem, bytes)| format!("{stem}={}", String::from_utf8_lossy(bytes)))
                .collect::<Vec<_>>()
                .join(",")
                .into_bytes()
        });
        world
            .replaced
            .push(("local.vault".into(), vec![joined(1), joined(2)], actual));
        world.replaced.push((
            "settings".into(),
            vec![b"settings-1".to_vec(), b"settings-2".to_vec()],
            std::fs::read(home.join("files/settings")).ok(),
        ));
    }
}
#[test]
fn faults_vault_write_resolves_one_complete_generation() {
    drive(&Vault);
}

// Immutable publication, promotion and staging settlement at every fault boundary.
struct ObjectPublication;
fn object_value(value: &str) -> crate::objects::Closure {
    crate::objects::Closure::new(BTreeMap::from([
        ("compose.yaml".into(), value.as_bytes().to_vec()),
        (
            "catalog/app/Dockerfile".into(),
            format!("FROM {value}").into_bytes(),
        ),
    ]))
    .expect("closure")
}
fn object_write(home: &Path, n: u8, value: &str) -> Result<(), RunError> {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let mut run = begin(&cx, &root, task(0), n, None, vec![], &mut fake)?;
    let value = object_value(value);
    crate::objects::Objects::publish_desired(&mut run, &value)?;
    crate::objects::Objects::promote(&mut run, value.digest())?;
    run.finish(WorkloadOutcome::Succeeded)?;
    run.settle(&cleanup(&cx), &mut fake).map(drop)
}
impl Scenario for ObjectPublication {
    fn name(&self) -> &'static str {
        "objects"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[Oracle::O3, Oracle::O4, Oracle::O6, Oracle::O7, Oracle::O10]
    }
    fn required(&self) -> &'static [Point] {
        &[
            Point::ObjectStageBefore,
            Point::ObjectStageCreated,
            Point::ObjectWriteBefore,
            Point::ObjectWriteAfter,
            Point::ObjectSealBefore,
            Point::ObjectSealAfter,
            Point::ObjectRenameBefore,
            Point::ObjectRenameAfter,
            Point::ObjectPublishBefore,
            Point::ObjectPublishAfter,
            Point::ObjectRefBefore,
            Point::ObjectRefAfter,
            Point::ObjectCleanupBefore,
            Point::ObjectCleanupAfter,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(0)]
    }
    fn prepare(&self, home: &Path) {
        object_write(home, 1, "old").expect("old object");
    }
    fn run(&self, home: &Path) -> Option<i32> {
        object_write(home, 2, "new").ok().map(|()| 0)
    }
    fn facts(&self, home: &Path, world: &mut World) {
        let root = match StateRoot::observe(&observe(), home) {
            Observed::Verified(root, _) => root,
            other => panic!("{other:?}"),
        };
        for role in [
            crate::objects::Role::Desired,
            crate::objects::Role::LastApplied,
        ] {
            let read = crate::objects::Objects::resolve(&observe(), &root, &task(0), role);
            let actual = read
                .verified()
                .map(|object| object.closure.digest().as_bytes().to_vec());
            world.replaced.push((
                format!("object {role:?}"),
                vec![
                    object_value("old").digest().as_bytes().to_vec(),
                    object_value("new").digest().as_bytes().to_vec(),
                ],
                actual,
            ));
        }
        // Recovery must remove all journalled staging, including a crash
        // before its receipt. Only complete desired/applied objects remain.
        if let Ok(entries) = std::fs::read_dir(root.path().join("staging")) {
            assert_eq!(
                entries.count(),
                0,
                "abandoned object staging survived recovery"
            );
        }
    }
}
#[test]
fn faults_objects_publish_promote_and_settle_preserve_complete_closures() {
    drive(&ObjectPublication);
}

struct ObjectReset;
impl Scenario for ObjectReset {
    fn name(&self) -> &'static str {
        "objects-reset"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[Oracle::O3, Oracle::O4, Oracle::O6, Oracle::O7, Oracle::O10]
    }
    fn required(&self) -> &'static [Point] {
        &[Point::ObjectRefBefore, Point::ObjectRefAfter]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(0), task(1)]
    }
    fn prepare(&self, home: &Path) {
        object_write(home, 1, "old").expect("old object");
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).expect("root");
        let mut fake = Fake::new(home);
        let mut sibling = begin(&cx, &root, task(1), 3, None, vec![], &mut fake).expect("sibling");
        crate::objects::Objects::publish_desired(&mut sibling, &object_value("old"))
            .expect("shared object");
        sibling
            .settle(&cleanup(&cx), &mut fake)
            .expect("settled sibling");
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).expect("root");
        let mut fake = Fake::new(home);
        let mut run = begin(&cx, &root, task(0), 2, None, vec![], &mut fake).ok()?;
        crate::objects::Objects::reset(&mut run).ok()?;
        run.settle(&cleanup(&cx), &mut fake).ok().map(|_| 0)
    }
    fn facts(&self, home: &Path, world: &mut World) {
        let root = match StateRoot::observe(&observe(), home) {
            Observed::Verified(root, _) => root,
            other => panic!("{other:?}"),
        };
        let mut actual = vec![];
        for role in [
            crate::objects::Role::Desired,
            crate::objects::Role::LastApplied,
        ] {
            let result = crate::objects::Objects::resolve(&observe(), &root, &task(0), role);
            match result {
                Observed::Absent(_) => actual.push(b'-'),
                Observed::Verified(object, _) => {
                    assert_eq!(object.closure, object_value("old"));
                    actual.push(b'+');
                }
                other => panic!("reset reference is unknown: {other:?}"),
            }
        }
        world.replaced.push((
            "object reset references".into(),
            vec![b"++".to_vec(), b"--".to_vec()],
            Some(actual),
        ));
        let sibling = crate::objects::Objects::resolve(
            &observe(),
            &root,
            &task(1),
            crate::objects::Role::Desired,
        );
        assert_eq!(
            sibling.verified().expect("sibling survived reset").closure,
            object_value("old")
        );
    }
}
#[test]
fn faults_objects_reset_is_atomic_and_preserves_sibling() {
    drive(&ObjectReset);
}

// --- Controlled discriminators for the effigy#618 failure classes. ---

/// A slow release exhausts injected cleanup before the next effect (effigy#618).
/// Recovery and an unfaulted prefix must finish that same work;
/// otherwise a later armed boundary disappears under load (effigy#868).
#[test]
fn faults_recovery_budget_settles_a_slow_release_the_injected_budget_refuses() {
    use crate::fault::{self, Fault};
    let home = home();
    let cx = mutate();
    let root = StateRoot::effects(&cx, &home.home()).expect("root");
    let mut fake = Fake::new(&home.home());
    slow_release();
    for n in 70..=73 {
        let acknowledge = if n == 71 { vec![id(70)] } else { vec![] };
        let mut run = begin(&cx, &root, task(10), n, None, acknowledge, &mut fake)
            .expect("begin, acknowledging the first run's residue");
        for key in [format!("slow-{n}"), format!("next-{n}")] {
            apply(&mut run, &fake, &key).expect("apply");
        }
        run.finish(WorkloadOutcome::Succeeded).expect("finish");
        fault::arm_point(Point::RequestBefore, Fault::Unknown);
        if n == 70 || n == 73 {
            fault::point(Point::RequestBefore).expect("deliver Unknown");
            assert!(
                fault::injected::<()>().is_some(),
                "consume injected Unknown"
            );
        }
        let budget = if n == 71 {
            recovery_cleanup(&cx)
        } else {
            cleanup(&cx)
        };
        if n == 70 || n == 73 {
            assert!(budget.operation().remaining().unwrap() <= Duration::from_secs(1));
            assert!(
                matches!(
                    run.settle(&budget, &mut fake),
                    Err(RunError::Interrupted(Interrupt::TimedOut { .. }))
                ),
                "injected cleanup must still refuse the slow release"
            );
        } else {
            run.settle(&budget, &mut fake)
                .expect("unfaulted cleanup reaches the next boundary");
            assert!(!fault::fired(), "the later target is still armed");
            fault::point(Point::RequestBefore).expect("later boundary");
            assert!(fault::fired(), "the later fault must be delivered");
        }
        fault::arm(None);
    }
    fault::hook(None);
}

/// Positive pin evidence: this launch's *recorded* anchor generation is alive
/// and the kernel reads it `Stopped`. A `kill` return is not evidence. Once the
/// group closes the anchor retires itself (effigy#520), so a pin can arrive
/// after it is gone, and this launch's recorded pid may then belong to another
/// process.
fn anchor_positively_stopped(anchor: &ProcessIdentity) -> bool {
    world::identity_alive(anchor)
        && matches!(
            crate::ident::process_state(anchor.pid),
            Observed::Verified(crate::ident::ProcessState::Stopped, _)
        )
}

/// A private stdin barrier; Drop releases it even during unwinding.
struct Held {
    process: Option<OwnedProcess>,
}
impl Held {
    fn new(process: OwnedProcess) -> Self {
        Self {
            process: Some(process),
        }
    }
    fn process(&self) -> &OwnedProcess {
        self.process.as_ref().expect("held workload")
    }
    /// Release the held workload and hand back the launch's own handle.
    fn release(mut self) -> OwnedProcess {
        self.send().expect("release the held workload");
        self.process.take().expect("held workload")
    }
    fn send(&mut self) -> std::io::Result<()> {
        let Some(process) = self.process.as_ref() else {
            return Ok(());
        };
        match process.write_stdin(b"\n", Instant::now() + Duration::from_secs(1)) {
            // A workload that already exited closed its own read end: the hold
            // is finished, not failed.
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            other => other,
        }
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        // Best effort while unwinding: the assertion that failed already names
        // the truth, and a second panic here would abort the test process.
        let _ = self.send();
    }
}

/// How the launch establishes the pin premise before settlement.
#[derive(Clone, Copy)]
enum Lead {
    /// The original fast exit leaves no workload to hold the anchor.
    FastExit,
    /// Held, released unpinned: the anchor retires itself (effigy#520).
    Natural,
    /// Held until its recorded anchor is positively stopped, then released.
    Pinned,
}
/// One custody launch, released only as far as `lead` allows, settled under an
/// armed `AnchorStop`. Returns whether the point delivered.
fn anchor_stop_delivery(home: &Home, lead: Lead) -> bool {
    let path = home.home();
    let cx = mutate();
    let root = StateRoot::effects(&cx, &path).expect("root");
    let mut fake = Fake::new(&path);
    let mut run = begin(
        &cx,
        &root,
        task(11),
        61,
        Some(Supervision::Inline),
        vec![],
        &mut fake,
    )
    .expect("begin");
    let body = if matches!(lead, Lead::FastExit) {
        "exit 0"
    } else {
        "read -r _"
    };
    let mut spec = workload(&path, body, finite());
    // The owner keeps the workload's stdin: that pipe is the private barrier.
    spec.terminal = Terminal::Pipe;
    let mut launched = Custody::spawn_all(&mut run, vec![spec], &cx).expect("spawn");
    let held = Held::new(launched.pop().expect("one workload"));
    let anchor = held.process().anchor().expect("recorded anchor");
    let mut anchors = AnchorPin::default();
    match lead {
        Lead::Pinned => {
            anchors.pin(std::slice::from_ref(held.process()));
            // The held workload keeps the group open until the pin is observed.
            wait_until("pinned anchor stopped", Duration::from_secs(2), || {
                anchor_positively_stopped(&anchor)
            });
        }
        Lead::FastExit => {
            // Establish the original fast-exit premise before attempting the pin.
            wait_until("fast-exit anchor retired", Duration::from_secs(5), || {
                !world::identity_alive(&anchor)
            });
            anchors.pin(std::slice::from_ref(held.process()));
            assert!(
                !anchor_positively_stopped(&anchor),
                "a retired anchor cannot be positive stopped evidence"
            );
        }
        Lead::Natural => {}
    }
    let mut process = held.release();
    let exit = match process.wait(&cx.operation().child(Duration::from_secs(1))) {
        Observed::Verified(exit, _) => exit,
        other => panic!("workload exit: {other:?}"),
    };
    process.complete(&mut run, exit).expect("complete");
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    if matches!(lead, Lead::Pinned) {
        // Past the anchor's poll bound a free-running anchor would be gone;
        // the pinned one must still sit at its settlement boundary.
        std::thread::sleep(Duration::from_millis(1200));
    } else {
        // The anchor retires itself within one poll of its group closing;
        // wait past that bound so the miss is deterministic, not lucky.
        wait_until("anchor self-retired", Duration::from_secs(5), || {
            !world::identity_alive(&anchor)
        });
    }
    crate::fault::arm(None);
    crate::fault::arm_nth(Point::AnchorStop, 0, crate::fault::Fault::Enospc);
    let settled = run.settle(&cleanup(&cx), &mut fake);
    let delivered = crate::fault::fired();
    crate::fault::arm(None);
    assert!(settled.is_ok(), "settle: {settled:?}");
    delivered
}

/// A retired anchor misses the stop points; a positively pinned one delivers.
/// The held workload avoids the fast-exit race observed in effigy#618 and #678.
#[test]
fn faults_anchor_retirement_delivery_is_pinned_to_settlement() {
    let fast = home();
    assert!(
        !anchor_stop_delivery(&fast, Lead::FastExit),
        "a fast-exit launch has no anchor left to pin, so it cannot deliver at AnchorStop"
    );
    let natural = home();
    assert!(
        !anchor_stop_delivery(&natural, Lead::Natural),
        "a self-retired anchor must not deliver at AnchorStop"
    );
    let pinned = home();
    assert!(
        anchor_stop_delivery(&pinned, Lead::Pinned),
        "the pinned anchor must deliver at AnchorStop"
    );
}

pub(super) fn by_name(name: &str) -> &'static dyn Scenario {
    let fixed: [&'static dyn Scenario; 8] = [
        &ManagedAttach,
        &Environment,
        &Residue,
        &OwnerLoss,
        &ObserveProbes,
        &Vault,
        &ObjectPublication,
        &ObjectReset,
    ];
    STEPS
        .iter()
        .map(|s| s as &dyn Scenario)
        .chain(fixed)
        .find(|scenario| scenario.name() == name)
        .unwrap_or_else(|| panic!("unknown scenario {name}"))
}

// --- Engine-lane scenarios: placeholders, not kernel work (§12.5). ---

// Gateway endpoint commits run against the user-owned daemon, watcher and
// live HTTP in effigy-gateway::server::endpoint_faults (test:gateway:proxy).
// Vault rotation is exercised by effigy-secrets::storage::tests::faults_engine_vault_rotation
// through the dependency fault hooks (test:secrets:vault).
// The out-of-process managed-run supervisor is killed at every kernel
// crossing it makes through the dependency fault hooks in
// effigy::runner::tests::managed_and_locking_tests::managed_supervisor_fault_tests
// (test:supervisor:faults).
