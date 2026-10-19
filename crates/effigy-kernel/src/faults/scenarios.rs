//! §12.1 scenarios the kernel layer expresses with strict fakes. Engine
//! scenarios are placeholders at the end, owned by their engine lanes.
use super::driver::{drive, Scenario};
use super::oracles::Oracle;
use super::world::{self, install_audits, World};
use super::*;
use crate::custody::{Custody, Lifetime, ProcessReceipt, ProcessSpec, Terminal};
use crate::fault::Point;
use crate::gate::{Gate, Wait};
use crate::id::*;
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
fn endpoint() -> ResourceId {
    ResourceId::endpoint(EndpointKind::Port, "8080").expect("endpoint id")
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
    fake: &mut Fake,
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
/// `Err(true)` is a refusal for unacknowledged residue (K5).
pub(super) fn rerun(home: &Path, owner: &ResourceId, n: u8) -> Result<(), bool> {
    let cx = mutate();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    match begin(&cx, &root, owner.clone(), n, None, vec![], &mut fake) {
        Err(RunError::PredecessorUnsettled { .. }) => Err(true),
        Ok(run) => run
            .settle(&cleanup(&cx), &mut fake)
            .map(drop)
            .map_err(|_| false),
        Err(_) => Err(false),
    }
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
        for step in self.steps {
            let specs = step
                .iter()
                .map(|body| workload(home, body, finite()))
                .collect();
            let mut processes = attempt!(run, &cx, fake, Custody::spawn_all(&mut run, specs, &cx));
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
        let _process = attempt!(run, &cx, fake, Custody::spawn(&mut run, spec, &cx));
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
    settle(run, &cx, fake).map(drop)
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
fn faults_environment_up_down_handover_with_concurrent_attach() {
    drive(&Environment);
}

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
        let outcome = run.settle(&cleanup(&cx), &mut fake).expect("settle");
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
/// Scheduler slack on top of deadline plus grace when sampling survivors.
const PROBE_SLACK: Duration = Duration::from_millis(250);
fn probe_bound() -> Duration {
    PROBE_DEADLINE + crate::probe::GRACE + PROBE_SLACK
}
fn wall_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis()
}
/// Samples announced probe processes once their evidence bound has passed.
fn probe_survivors(home: &Path) -> Vec<world::Proc> {
    let Some(started) = std::fs::read_to_string(aux(home).join("probe-started"))
        .ok()
        .and_then(|t| t.trim().parse::<u128>().ok())
    else {
        return vec![];
    };
    let due = started + probe_bound().as_millis();
    let now = wall_ms();
    if now < due {
        std::thread::sleep(Duration::from_millis((due - now) as u64));
    }
    world::pids(&aux(home).join("probe-pids"))
        .into_iter()
        .map(|pid| world::proc(pid, b"EFFIGY_PROBE_NONCE="))
        .collect()
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
        std::fs::write(aux(home).join("probe-started"), wall_ms().to_string()).ok()?;
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

// --- Endpoint reserve and commit. ---

#[derive(Debug)]
struct Endpoint;
fn claim_state(home: &Path) -> Option<String> {
    let cx = observe();
    let Observed::Verified(root, _) = StateRoot::observe(&cx, home) else {
        return None;
    };
    let Observed::Verified(reader, _) = Store::read(&cx, &root) else {
        return None;
    };
    reader
        .conn()
        .query_row(
            "SELECT state FROM claims WHERE resource=?1",
            [endpoint().name().to_string()],
            |r| r.get(0),
        )
        .ok()
}
impl Scenario for Endpoint {
    fn name(&self) -> &'static str {
        "endpoint"
    }
    fn oracles(&self) -> &'static [Oracle] {
        &[
            Oracle::O1a,
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
            Point::ClaimReserveBefore,
            Point::ClaimReserveAfter,
            Point::ClaimReleaseBefore,
            Point::ClaimReleaseAfter,
            Point::AdapterBefore,
            Point::AdapterAfter,
        ]
    }
    fn owners(&self) -> Vec<ResourceId> {
        vec![task(10), task(11)]
    }
    fn prepare(&self, home: &Path) {
        install_audits(home);
    }
    fn run(&self, home: &Path) -> Option<i32> {
        let cx = mutate();
        let root = StateRoot::effects(&cx, home).ok()?;
        let fake = &mut Fake::new(home);
        let mut run = begin(&cx, &root, task(10), 50, None, vec![], fake).ok()?;
        let token = attempt!(
            run,
            &cx,
            fake,
            Claims::reserve(&mut run, &[endpoint()], Duration::from_secs(600))
        );
        attempt!(run, &cx, fake, Claims::commit(&mut run, &token, 1));
        attempt!(run, &cx, fake, apply(&mut run, fake, "route-8080"));
        attempt!(run, &cx, fake, run.finish(WorkloadOutcome::Succeeded));
        let exit = settle(run, &cx, fake);
        // A held, live or quarantined claim refuses a second reservation;
        // a jumped wall clock never lifts a boot-clock quarantine.
        let before = claim_state(home);
        let mut second = begin(&cx, &root, task(11), 51, None, vec![], fake).ok()?;
        let reserved = Claims::reserve(&mut second, &[endpoint()], Duration::ZERO);
        let held = matches!(before.as_deref(), Some("reserved" | "live" | "quarantined"));
        assert!(
            !(held && reserved.is_ok()),
            "claim {before:?} reserved twice"
        );
        let _ = second.settle(&cleanup(&cx), fake);
        exit
    }
}
#[test]
fn faults_endpoint_reserve_and_commit() {
    drive(&Endpoint);
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
            Point::ObjectRootSealBefore,
            Point::ObjectRootSealAfter,
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

pub(super) fn by_name(name: &str) -> &'static dyn Scenario {
    let fixed: [&'static dyn Scenario; 9] = [
        &ManagedAttach,
        &Environment,
        &Residue,
        &OwnerLoss,
        &ObserveProbes,
        &Endpoint,
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

#[test]
#[ignore = "engine lane (environment): real backend handover on Colima, Docker Desktop and Linux Docker"]
fn faults_engine_real_environment_handover() {}
#[test]
#[ignore = "engine lane (environment, gateway): legacy import (§11) of host sidecars, gateway state and routes"]
fn faults_engine_legacy_import() {}
#[test]
#[ignore = "engine lane (gateway): endpoint commit through the user-owned daemon and its route effects"]
fn faults_engine_gateway_endpoint_commit() {}
// Vault rotation is exercised by effigy-secrets::storage::tests::faults_engine_vault_rotation
// through the dependency fault hooks (test:secrets:vault).
#[test]
#[ignore = "engine lane (supervisor): fault points inside the out-of-process detached supervisor binary"]
fn faults_engine_detached_supervisor_internal_points() {}
