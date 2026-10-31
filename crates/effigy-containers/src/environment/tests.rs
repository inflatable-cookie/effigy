//! Engine proofs (contract 053 §8): strict-fake backends, the transition
//! matrix against live, interrupted and unknown holders, and fault injection
//! at every acquisition and publication boundary.

use super::*;
use effigy_kernel::ctx::{CancelToken, EffectClass, Placement};
use effigy_kernel::gate::Wait;
use effigy_kernel::gate::{ForegroundDiag, HolderDiag};
use effigy_kernel::id::ResourceId;
use effigy_kernel::id::{
    CheckoutId, CheckoutKind, ExecutionTarget, InvocationSpec, ResolvedInvocation, Route, RunId,
    Surface, TaskId,
};
use effigy_kernel::ident::ProcessState;
use effigy_kernel::objects::Closure;
use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use effigy_kernel::outcome::{CleanupOutcome, Invocation, WorkloadOutcome};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{AttachMode, BeginOpts, Liveness, Run};
use effigy_kernel::store::Store;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The busy sentence is written from the gate's own holder diagnostics
/// (effigy#313), never from a caller-supplied string, and it suggests one
/// command the CLI accepts.
#[test]
fn the_busy_refusal_text_names_the_holder_the_gate_recorded() {
    let checkout = checkout();
    let evidence = || Evidence::new("gate body probe".to_owned(), None).expect("evidence");
    let holder = HolderDiag {
        foreground: Some(ForegroundDiag {
            root: false,
            owner: ResourceId::environment(checkout, PROJECT).expect("owner"),
            task: TaskId::new(checkout, "api", Surface::Published, "dev", None).expect("task"),
            gate: ResourceId::environment(checkout, PROJECT).expect("gate"),
        }),
        run: Some("agqw465xzsu66h77ejdxon3hde".to_owned()),
        pid: 99290,
        start: None,
        boot: None,
        argv0: None,
        state: Observed::Verified(ProcessState::Active, evidence()),
    };
    let text = reconcile::busy_text(
        PROJECT,
        &Observed::Verified(holder, evidence()),
        Some(std::time::Duration::from_millis(1500)),
    );
    for needle in [
        "environment `acme-stack` is busy",
        "run agqw465xzsu66h77ejdxon3hde",
        "pid 99290",
        "task `api/dev`",
        "and the wait ended after 1.5s",
        "`effigy container acme-stack status`",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }

    // An unreadable gate body is named by category only, never by detail.
    let unknown = reconcile::busy_text(PROJECT, &Observed::Absent(AbsenceProof::NoGateInode), None);
    assert!(
        unknown.contains("a run whose gate body is absent is transitioning it"),
        "{unknown}"
    );
    assert!(
        unknown.contains("`effigy container acme-stack status`"),
        "{unknown}"
    );
}

mod adapters;
mod creation;
mod faults;
mod guest_publication;

const PROJECT: &str = "acme-stack";

pub(super) fn home() -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::new("effigy-environment-").expect("private home")
}

pub(super) fn root(home: &Path) -> StateRoot {
    let operation = reconcile::operation(EffectClass::Mutate, CancelToken::default(), RUN_BUDGET);
    StateRoot::effects(&operation.effects().expect("effects"), home).expect("state root")
}

pub(super) fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16])
}

pub(super) fn env(name: &str) -> EnvRef {
    EnvRef::new(checkout(), name).expect("environment identity")
}

pub(super) fn closure(tag: &str) -> Closure {
    Closure::new(BTreeMap::from([(
        "compose.yml".to_owned(),
        format!("services: {{app: {{image: {tag}}}}}\n").into_bytes(),
    )]))
    .expect("closure")
}

pub(super) fn selection() -> BackendSelection {
    BackendSelection {
        kind: BackendKind::Colima,
        engine: crate::BackendId::colima_nerdctl(),
        context: "fixture".to_owned(),
        reason: BackendReason::ManifestDriver("colima".to_owned()),
    }
}

pub(super) fn policy(root: &Path) -> crate::EffectiveContainerPolicy {
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    crate::EffectiveContainerPolicy {
        render: None,
        repo_root: root.to_path_buf(),
        name: "stack".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: crate::EffectiveComposeSource::Generated,
        compose_files: vec![root.join("compose.yml")],
        compose_file_display: "compose.yml".to_owned(),
        managed_volumes: vec![],
        borrowed_volumes: vec![],
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: PROJECT.to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: None,
        dns_base_domain: None,
        runtime_host_key: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: vec![],
        service_aliases: vec![],
        declared_ports: vec![],
        ports_declared_explicitly: false,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: vec![],
    }
}

/// The bounded cleanup context a test's own run settlement uses; production
/// makes a fresh one per settlement (effigy#447).
pub(super) fn settlement(ctx: &EnvContext<'_>) -> effigy_kernel::ctx::CleanupContext {
    ctx.cleanup().expect("cleanup budget")
}

/// A holder: one task run of the same checkout, owned by its own gate.
pub(super) fn holder<'r>(ctx: &EnvContext<'r>, task: &str, n: u8) -> Run<'r> {
    let task = TaskId::new(checkout(), "", Surface::Published, task, None).expect("task id");
    let invocation = ResolvedInvocation::new(InvocationSpec {
        task: task.clone(),
        route: Route::Task,
        argv: vec![],
        cwd: String::new(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner: task.resource(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("invocation");
    Run::begin(
        &ctx.cx,
        ctx.root,
        &invocation,
        BeginOpts {
            id: RunId::new(2, [n; 10]).expect("run id"),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: Invocation::validated(vec!["effigy".into(), "task".into()], |_| true)
                .expect("next"),
            custody: None,
        },
        &settlement(ctx),
        &mut EnvSettlement,
    )
    .expect("holder admitted")
}

/// A strict fake backend for one Compose project. It models running state,
/// records every call, refuses any other project and replays queued
/// outcomes. A shared log file lets a killed child's calls be audited.
#[derive(Clone)]
pub(super) struct Fake {
    selection: BackendSelection,
    state: Rc<RefCell<FakeState>>,
}

struct FakeState {
    running: bool,
    calls: Vec<String>,
    outcomes: VecDeque<Result<(), BackendError>>,
    log: Option<PathBuf>,
}

impl Fake {
    pub(super) fn new() -> Self {
        Self::with_log(None)
    }

    pub(super) fn with_log(log: Option<PathBuf>) -> Self {
        let running = log.as_deref().map(Self::log_running).unwrap_or(false);
        Self {
            selection: selection(),
            state: Rc::new(RefCell::new(FakeState {
                running,
                calls: vec![],
                outcomes: VecDeque::new(),
                log,
            })),
        }
    }

    /// The last modelled call in a log decides whether workloads run.
    pub(super) fn log_running(log: &Path) -> bool {
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .rev()
            .find_map(|line| match line {
                "apply" => Some(true),
                "stop" => Some(false),
                _ => None,
            })
            .unwrap_or(false)
    }

    pub(super) fn queue(&self, outcome: Result<(), BackendError>) {
        self.state.borrow_mut().outcomes.push_back(outcome);
    }

    pub(super) fn calls(&self) -> Vec<String> {
        self.state.borrow().calls.clone()
    }

    fn call(&self, action: &str, project: &str) -> Result<(), BackendError> {
        assert_eq!(project, PROJECT, "fake refuses an unmodelled project");
        let mut state = self.state.borrow_mut();
        let outcome = state.outcomes.pop_front().unwrap_or(Ok(()));
        if !matches!(outcome, Err(BackendError::NotStarted(_))) {
            state.calls.push(action.to_owned());
            if let Some(log) = &state.log {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log)
                    .expect("fake log");
                writeln!(file, "{action}").expect("fake log write");
                file.sync_all().expect("fake log sync");
            }
            match action {
                "apply" => state.running = true,
                "stop" => state.running = false,
                _ => {}
            }
        }
        outcome
    }

    pub(super) fn apply(&self) -> Result<(), BackendError> {
        self.call("apply", PROJECT)
    }

    pub(super) fn stop(&self) -> Result<(), BackendError> {
        self.call("stop", PROJECT)
    }
}

impl EnvironmentBackend for Fake {
    fn selection(&self) -> &BackendSelection {
        &self.selection
    }
    fn inventory(&mut self, _: &Path, project: &str) -> Observed<Vec<Workload>> {
        panic!("strict fake: unmodelled inventory of `{project}`")
    }
}

/// The production shape of one bring-up: the apply run, the command's own
/// integrations, then the activation run.
pub(super) fn bring_up(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    fake: &Fake,
    render: &Closure,
    retention: Retention,
    holders: &[(RunId, AttachMode)],
) -> Result<Applied, EnvError> {
    let brought_up = reconcile(
        ctx,
        env,
        Up {
            render: Some(render),
            project: PROJECT,
            backend: &selection(),
            retention,
            workdir: std::path::Path::new("."),
            primary_service: "app",
            compose_files: &[],
        },
        || fake.apply(),
    )?;
    activate(ctx, env, holders)?;
    Ok(brought_up.applied)
}

/// What one create effect needs from its caller, for the fixture checkout.
pub(super) fn create_request() -> crate::environment::CreationRequest<'static> {
    crate::environment::CreationRequest {
        workdir: std::path::Path::new("."),
        project: PROJECT,
        primary_service: "app",
        compose_files: &[],
    }
}

pub(super) fn take_down(
    ctx: &EnvContext<'_>,
    env: &EnvRef,
    op: Operation,
    fake: &Fake,
    exempt: Option<RunId>,
) -> Result<(), EnvError> {
    down(ctx, env, op, &selection(), PROJECT, exempt, || fake.stop())
}

pub(super) fn record(ctx: &EnvContext<'_>, env: &EnvRef) -> EnvRecord {
    match EnvRecord::read(&ctx.observe(), ctx.root, &env.resource) {
        Observed::Verified(record, _) => record,
        other => panic!("record unreadable: {other:?}"),
    }
}

/// O2, O3 and O6: at most one unresolved run per owner, each interrupted
/// (no live holder of its gate) with a recovery command, and an intact store.
pub(super) fn assert_store_sound(ctx: &EnvContext<'_>) {
    let reader = match Store::read(&ctx.observe(), ctx.root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("store unreadable: {other:?}"),
    };
    let runs =
        match effigy_kernel::run::unclosed(&reader, effigy_kernel::id::Scope::Checkout(checkout()))
        {
            Observed::Verified(runs, _) => runs,
            other => panic!("journal unreadable: {other:?}"),
        };
    let mut owners = std::collections::BTreeSet::new();
    for run in &runs {
        let owner = run
            .invocation
            .as_ref()
            .expect("decodable invocation")
            .spec()
            .owner
            .clone();
        assert!(
            owners.insert(owner.name().to_string()),
            "two unresolved runs: {runs:?}"
        );
        assert!(
            matches!(
                effigy_kernel::gate::Gate::probe(&ctx.observe(), ctx.root, &owner),
                Observed::Verified(effigy_kernel::gate::GateState::Free, _) | Observed::Absent(_)
            ),
            "an unclosed run still holds its gate: {run:?}"
        );
    }
    Store::open(&ctx.cx.child(std::time::Duration::from_secs(10)), ctx.root)
        .expect("store opens")
        .check_integrity()
        .expect("integrity");
}

#[test]
fn record_read_is_pure_and_absent_without_a_store() {
    let home = home();
    let operation = reconcile::operation(
        EffectClass::Observe,
        CancelToken::default(),
        std::time::Duration::from_secs(5),
    );
    let observe = operation.observe().expect("observe");
    assert!(matches!(
        StateRoot::observe(&observe, home.path()),
        Observed::Absent(_)
    ));
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    assert!(matches!(
        EnvRecord::read(&ctx.observe(), &root, &env("stack").resource),
        Observed::Absent(_)
    ));
}

/// Q-011: an unreadable holder list is not held. The apply still runs and the
/// engine reports it, so the runner can warn; a known empty list stays quiet.
#[test]
fn an_unreadable_holder_list_applies_and_says_so() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let render = closure("one");
    let backend = selection();
    let up = || Up {
        render: Some(&render),
        project: PROJECT,
        backend: &backend,
        retention: Retention::Held,
        workdir: std::path::Path::new("."),
        primary_service: "app",
        compose_files: &[],
    };
    let fake = Fake::new();
    let observer = |_: &EnvRun<'_>| {
        Observed::Unknown(UnknownReason::StoreUnreadable("fake observer".to_owned()))
    };
    let applied =
        super::ops::reconcile_observing(&ctx, &stack, up(), || fake.apply(), observer).unwrap();
    assert_eq!(
        applied.applied,
        Applied::ReconciledUnreadableHolders(UnknownReason::StoreUnreadable(
            "fake observer".to_owned()
        ))
    );
    assert_eq!(fake.calls(), ["apply"]);
    activate(&ctx, &stack, &[]).unwrap();
    assert!(record(&ctx, &stack).active());

    let quiet = Fake::new();
    let empty = |_: &EnvRun<'_>| {
        let evidence = Evidence::new("fake observer".to_owned(), None).expect("evidence");
        Observed::Verified(Vec::new(), evidence)
    };
    assert_eq!(
        super::ops::reconcile_observing(&ctx, &stack, up(), || quiet.apply(), empty)
            .unwrap()
            .applied,
        Applied::Reconciled
    );
    assert_eq!(quiet.calls(), ["apply"]);
    assert_store_sound(&ctx);
}

#[test]
fn up_applies_once_and_keeps_an_explicit_retention() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let render = closure("one");
    assert_eq!(
        bring_up(&ctx, &stack, &fake, &render, Retention::Persistent, &[]).unwrap(),
        Applied::Reconciled
    );
    let applied = record(&ctx, &stack);
    assert!(applied.active());
    assert_eq!(applied.desired.as_deref(), Some(render.digest()));
    assert_eq!(applied.engine().unwrap().retention, Retention::Persistent);
    // Prepare records the backend (#230); a bring-up never writes it.
    assert!(applied.recorded_backend().is_none());
    // One apply path: a second bring-up converges the backend again, and
    // Compose decides what (if anything) to recreate.
    assert_eq!(
        bring_up(&ctx, &stack, &fake, &render, Retention::Held, &[]).unwrap(),
        Applied::Reconciled
    );
    assert_eq!(fake.calls(), ["apply", "apply"]);
    // A task's ensure never demotes an explicit up's persistence.
    assert_eq!(
        record(&ctx, &stack).engine().unwrap().retention,
        Retention::Persistent
    );
    assert_store_sound(&ctx);
}

#[test]
fn backend_outcomes_are_journalled_without_manufacturing_success() {
    for (outcome, applied_calls) in [
        // The engine may have acted: the call is recorded, and the retry
        // converges the backend again.
        (BackendError::Uncertain("timed out".into()), 2),
        // Verified non-occurrence: only the retry's call reached the backend.
        (BackendError::NotStarted("colima: not found".into()), 1),
    ] {
        let home = home();
        let root = root(home.path());
        let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
        let stack = env("stack");
        let fake = Fake::new();
        fake.queue(Err(outcome));
        let render = closure("one");
        assert!(matches!(
            bring_up(&ctx, &stack, &fake, &render, Retention::Held, &[]),
            Err(EnvError::Backend(_))
        ));
        let failed = record(&ctx, &stack);
        assert!(!failed.active());
        bring_up(&ctx, &stack, &fake, &render, Retention::Held, &[]).unwrap();
        assert_eq!(fake.calls().len(), applied_calls);
        assert!(record(&ctx, &stack).active());
        assert_store_sound(&ctx);
    }
}

/// The matrix: every transition against a live, an interrupted and an
/// unknown holder. Live and unknown holders refuse every transition with
/// zero backend calls (O11); an interrupted holder is settled by the
/// transition first, which then acts exactly once.
#[test]
fn transitions_refuse_against_live_interrupted_and_unknown_holders() {
    for liveness in ["live", "interrupted", "unknown"] {
        for transition in ["recreate", "down", "handover", "reset", "reap"] {
            let home = home();
            let root = root(home.path());
            let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
            let stack = env("stack");
            let fake = Fake::new();
            bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
            let mut task = holder(&ctx, "dev", 1);
            hold(&mut task, &ctx, &stack, Wait::FailFast, true).unwrap();
            let holder_run = task.id();
            let mut task = match liveness {
                "live" => Some(task),
                "interrupted" => {
                    // The holder's process died: its gate is free, its row open.
                    drop(task);
                    None
                }
                _ => {
                    // An effect the holder cannot prove settled keeps its
                    // attachment as residue: liveness becomes unknown.
                    let ticket = task
                        .intend(effigy_kernel::run::EffectSpec {
                            target: task.fence().clone(),
                            kind: "fixture".into(),
                            payload: serde_json::Value::Null,
                            settlement: effigy_kernel::run::Settlement::Release(
                                serde_json::Value::Null,
                            ),
                            required: false,
                        })
                        .unwrap();
                    task.applied(&ticket, serde_json::Value::Bool(true))
                        .unwrap();
                    task.finish(WorkloadOutcome::Succeeded).unwrap();
                    assert_eq!(
                        task.settle(&settlement(&ctx), &mut EnvSettlement)
                            .unwrap()
                            .cleanup(),
                        CleanupOutcome::Incomplete
                    );
                    None
                }
            };
            let before = fake.calls().len();
            let result = match transition {
                "recreate" => bring_up(&ctx, &stack, &fake, &closure("two"), Retention::Held, &[])
                    .map(|applied| {
                        if liveness == "interrupted" {
                            assert_eq!(applied, Applied::Reconciled);
                            return;
                        }
                        match applied {
                            // A live or unknown holder keeps the running
                            // stack, and the up names it (effigy#371 F8).
                            Applied::HeldByRuns(holders) => assert_eq!(
                                holders,
                                vec![holder_run],
                                "the deferred apply names its holder"
                            ),
                            other => panic!("recreate against a {liveness} holder: {other:?}"),
                        }
                    }),
                "down" => take_down(&ctx, &stack, Operation::Down, &fake, None),
                "handover" => take_down(&ctx, &stack, Operation::Handover, &fake, None),
                "reset" => reset(&ctx, &stack, &selection(), PROJECT, || fake.stop()),
                _ => reap_if_idle(&ctx, &stack, |run| {
                    run.stop_with(&selection(), PROJECT, None, || fake.stop())
                })
                .map(|idle| {
                    let stopped = idle == Idle::Stopped;
                    assert_eq!(stopped, liveness == "interrupted", "reap: {idle:?}");
                }),
            };
            if liveness == "interrupted" {
                // The transition settles the dead holder as gate holder
                // (I-A2), then acts exactly once.
                result.unwrap_or_else(|error| panic!("{transition}: {error}"));
                assert_eq!(fake.calls().len(), before + 1, "{transition}");
                assert!(!record(&ctx, &stack).protected());
            } else {
                if transition != "recreate" && transition != "reap" {
                    assert!(
                        matches!(result, Err(EnvError::HeldBy { .. })),
                        "{transition} against a {liveness} holder: {result:?}"
                    );
                } else {
                    result.unwrap();
                }
                assert_eq!(
                    fake.calls().len(),
                    before,
                    "{transition} reached the backend against a {liveness} holder"
                );
                assert!(
                    record(&ctx, &stack).protected(),
                    "{liveness} holder lost protection"
                );
            }
            if liveness == "live" && transition == "recreate" {
                // The edit an explicit `up` reported as held applies once its
                // holder releases, without a further declaration change.
                task.take();
                assert_eq!(
                    bring_up(&ctx, &stack, &fake, &closure("two"), Retention::Held, &[]).unwrap(),
                    Applied::Reconciled
                );
                assert_eq!(fake.calls().len(), before + 1, "{transition}");
            }
            drop(task);
            assert_store_sound_except_holders(&ctx);
        }
    }
}

/// O3 holds for environment runs; a deliberately abandoned holder may stay.
fn assert_store_sound_except_holders(ctx: &EnvContext<'_>) {
    let reader = match Store::read(&ctx.observe(), ctx.root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("store unreadable: {other:?}"),
    };
    match effigy_kernel::run::unclosed(&reader, effigy_kernel::id::Scope::Checkout(checkout())) {
        Observed::Verified(runs, _) => assert!(runs.iter().all(|run| run
            .invocation
            .as_ref()
            .is_some_and(|invocation| invocation.spec().owner.kind()
                == effigy_kernel::id::ResourceKind::Task))),
        other => panic!("journal unreadable: {other:?}"),
    }
}

#[test]
fn a_holder_s_own_stop_is_exempt_only_once_it_is_settling() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let mut task = holder(&ctx, "dev", 1);
    // The run asked for the ensure; the apply run attaches it.
    bring_up(
        &ctx,
        &stack,
        &fake,
        &closure("one"),
        Retention::Held,
        &[(
            task.id(),
            AttachMode::Exclusive {
                stop_on_release: true,
            },
        )],
    )
    .unwrap();
    assert!(record(&ctx, &stack).protected());
    assert!(matches!(
        take_down(&ctx, &stack, Operation::Down, &fake, Some(task.id())),
        Err(EnvError::HeldBy { .. })
    ));
    assert_eq!(fake.calls(), ["apply"]);
    task.finish(WorkloadOutcome::Succeeded).unwrap();
    take_down(&ctx, &stack, Operation::Down, &fake, Some(task.id())).unwrap();
    assert_eq!(fake.calls(), ["apply", "stop"]);
    assert_eq!(record(&ctx, &stack).state, EnvState::Stopped);
    assert_eq!(
        task.settle(&settlement(&ctx), &mut EnvSettlement)
            .unwrap()
            .cleanup(),
        CleanupOutcome::Complete
    );
    assert!(!record(&ctx, &stack).protected());
    assert_store_sound(&ctx);
}

#[test]
fn idle_stop_is_holder_based() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    let reap = |fake: &Fake| {
        reap_if_idle(&ctx, &stack, |run| {
            run.stop_with(&selection(), PROJECT, None, || fake.stop())
        })
        .unwrap()
    };
    let mut first = holder(&ctx, "test", 1);
    let held = hold(&mut first, &ctx, &stack, Wait::FailFast, true).unwrap();
    assert!(matches!(
        held.mode,
        AttachMode::Exclusive {
            stop_on_release: true
        }
    ));
    let mut second = holder(&ctx, "lint", 2);
    hold(&mut second, &ctx, &stack, Wait::FailFast, true).unwrap();
    first.finish(WorkloadOutcome::Succeeded).unwrap();
    first.settle(&settlement(&ctx), &mut EnvSettlement).unwrap();
    assert_eq!(reap(&fake), Idle::Kept("holders remain"));
    second.finish(WorkloadOutcome::Succeeded).unwrap();
    second
        .settle(&settlement(&ctx), &mut EnvSettlement)
        .unwrap();
    assert_eq!(reap(&fake), Idle::Stopped);
    assert_eq!(fake.calls(), ["apply", "stop"]);
    assert_eq!(reap(&fake), Idle::Kept("not active"));
    // A persistent environment is borrowed and left running.
    let fake = Fake::new();
    let other = env("persistent");
    bring_up(
        &ctx,
        &other,
        &fake,
        &closure("one"),
        Retention::Persistent,
        &[],
    )
    .unwrap();
    let mut borrower = holder(&ctx, "build", 3);
    let held = hold(&mut borrower, &ctx, &other, Wait::FailFast, true).unwrap();
    assert!(matches!(held.mode, AttachMode::Borrow));
    borrower.finish(WorkloadOutcome::Succeeded).unwrap();
    borrower
        .settle(&settlement(&ctx), &mut EnvSettlement)
        .unwrap();
    assert_eq!(
        reap_if_idle(&ctx, &other, |_| panic!("persistent stack stopped")).unwrap(),
        Idle::Kept("persistent")
    );
    assert_eq!(fake.calls(), ["apply"]);
    assert_store_sound(&ctx);
}

#[test]
fn a_stopped_or_unrecorded_environment_cannot_be_held() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let mut task = holder(&ctx, "dev", 1);
    assert!(matches!(
        hold(&mut task, &ctx, &stack, Wait::FailFast, false),
        Err(HoldError::NotActive)
    ));
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    take_down(&ctx, &stack, Operation::Down, &fake, None).unwrap();
    assert!(matches!(
        hold(&mut task, &ctx, &stack, Wait::FailFast, true),
        Err(HoldError::NotActive)
    ));
    task.settle(&settlement(&ctx), &mut EnvSettlement).unwrap();
    assert_store_sound(&ctx);
}

#[test]
fn reset_drops_only_its_own_render_references() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let sibling = env("hybrid");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    bring_up(&ctx, &sibling, &fake, &closure("two"), Retention::Held, &[]).unwrap();
    reset(&ctx, &stack, &selection(), PROJECT, || fake.stop()).unwrap();
    let reset_record = record(&ctx, &stack);
    assert_eq!(reset_record.desired, None);
    assert_eq!(reset_record.state, EnvState::Stopped);
    let kept = record(&ctx, &sibling);
    assert_eq!(kept.desired.as_deref(), Some(closure("two").digest()));
    assert!(kept.active());
    assert_store_sound(&ctx);
}

/// A transition stalled between permit and backend excludes a concurrent
/// attach: the attacher waits on `Gate(Env)` and never sees the environment
/// it is stopping as active.
#[test]
fn a_stalled_transition_excludes_a_concurrent_attach() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    let (inside_tx, inside_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let path = home.path().to_path_buf();
    let attacher = std::thread::spawn(move || {
        inside_rx.recv().unwrap();
        let root = self::root(&path);
        let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
        let mut task = holder(&ctx, "late", 9);
        let busy = hold(&mut task, &ctx, &env("stack"), Wait::FailFast, true);
        assert!(matches!(busy, Err(HoldError::Run(_))), "{busy:?}");
        release_tx.send(()).unwrap();
        let waited = hold(
            &mut task,
            &ctx,
            &env("stack"),
            Wait::Until(std::time::Instant::now() + std::time::Duration::from_secs(20)),
            true,
        );
        assert!(matches!(waited, Err(HoldError::NotActive)), "{waited:?}");
        task.settle(&settlement(&ctx), &mut EnvSettlement).unwrap();
    });
    down(
        &ctx,
        &stack,
        Operation::Down,
        &selection(),
        PROJECT,
        None,
        || {
            inside_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            fake.stop()
        },
    )
    .unwrap();
    attacher.join().unwrap();
    assert_eq!(fake.calls(), ["apply", "stop"]);
    assert_store_sound(&ctx);
}

#[test]
fn a_transition_settles_an_interrupted_holder_before_acting() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    let mut task = holder(&ctx, "dev", 1);
    hold(&mut task, &ctx, &stack, Wait::FailFast, true).unwrap();
    drop(task);
    let observed = record(&ctx, &stack);
    assert!(matches!(&observed.holders, Observed::Verified(holders, _)
        if matches!(holders[0].liveness, Observed::Verified(Liveness::Interrupted, _))));
    // A live successor of the same task settles its predecessor itself.
    let successor = holder(&ctx, "dev", 2);
    assert!(!record(&ctx, &stack).protected());
    successor
        .settle(&settlement(&ctx), &mut EnvSettlement)
        .unwrap();
    // A dead session with no successor is settled by the next transition.
    let mut session = Session::begin(&ctx, &stack, "exec").unwrap();
    hold(session.run_mut(), &ctx, &stack, Wait::FailFast, true).unwrap();
    assert!(matches!(
        take_down(&ctx, &stack, Operation::Down, &fake, None),
        Err(EnvError::HeldBy { .. })
    ));
    std::mem::forget(session);
    // `forget` leaves the session's gate descriptor open in this process,
    // so it still reads live: the down keeps refusing.
    assert!(matches!(
        take_down(&ctx, &stack, Operation::Down, &fake, None),
        Err(EnvError::HeldBy { .. })
    ));
    assert_eq!(fake.calls(), ["apply"]);
    assert_store_sound_except_holders(&ctx);
}

#[test]
fn a_session_holds_for_a_command_without_a_task_run() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    let mut first = Session::begin(&ctx, &stack, "exec").unwrap();
    let mut second = Session::begin(&ctx, &stack, "shell").unwrap();
    assert_ne!(first.id(), second.id(), "sessions never share a gate");
    hold(first.run_mut(), &ctx, &stack, Wait::FailFast, true).unwrap();
    hold(second.run_mut(), &ctx, &stack, Wait::FailFast, true).unwrap();
    assert!(matches!(
        take_down(&ctx, &stack, Operation::Down, &fake, None),
        Err(EnvError::HeldBy { .. })
    ));
    assert_eq!(first.close(), CleanupOutcome::Complete);
    let reap = || {
        reap_if_idle(&ctx, &stack, |run| {
            run.stop_with(&selection(), PROJECT, None, || fake.stop())
        })
        .unwrap()
    };
    assert_eq!(reap(), Idle::Kept("holders remain"));
    drop(second);
    assert_eq!(reap(), Idle::Stopped);
    assert_eq!(fake.calls(), ["apply", "stop"]);
    assert_store_sound(&ctx);
}

/// An `up` interrupted before activation leaves nothing to clean up: the
/// next `up` after a manifest edit applies the edit, with no refusal.
#[test]
fn an_interrupted_up_after_a_manifest_edit_applies_on_the_next_up() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    assert_eq!(
        reconcile(
            &ctx,
            &stack,
            Up {
                render: Some(&closure("one")),
                project: PROJECT,
                backend: &selection(),
                retention: Retention::Persistent,
                workdir: std::path::Path::new("."),
                primary_service: "app",
                compose_files: &[],
            },
            || fake.apply(),
        )
        .unwrap()
        .applied,
        Applied::Reconciled
    );
    // Interrupted here: no activation ran.
    assert!(!record(&ctx, &stack).active());
    // The declaration changed; the next up applies it.
    let edited = closure("two");
    assert_eq!(
        bring_up(&ctx, &stack, &fake, &edited, Retention::Persistent, &[]).unwrap(),
        Applied::Reconciled
    );
    let applied = record(&ctx, &stack);
    assert!(applied.active());
    assert_eq!(applied.desired.as_deref(), Some(edited.digest()));
    assert_eq!(fake.calls(), ["apply", "apply"]);
    assert_store_sound(&ctx);
}

/// `hold` asks the runtime whether the project's containers are running, so a
/// stack deleted behind Effigy's back is never "held" as if it were up.
#[test]
fn a_hold_reports_not_active_when_the_runtime_reports_nothing_running() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[]).unwrap();
    assert!(record(&ctx, &stack).active());
    let mut task = holder(&ctx, "dev", 1);
    assert!(matches!(
        hold(&mut task, &ctx, &stack, Wait::FailFast, false),
        Err(HoldError::NotActive)
    ));
    // The bring-up that follows starts and attaches it.
    assert_eq!(
        bring_up(
            &ctx,
            &stack,
            &fake,
            &closure("one"),
            Retention::Held,
            &[(task.id(), AttachMode::Borrow)],
        )
        .unwrap(),
        Applied::Reconciled
    );
    assert!(record(&ctx, &stack).protected());
    task.finish(WorkloadOutcome::Succeeded).unwrap();
    task.settle(&settlement(&ctx), &mut EnvSettlement).unwrap();
    assert_store_sound(&ctx);
}

#[test]
fn backend_selection_records_its_reason_and_unknown_engines_refuse() {
    let recorded = selection();
    let metadata = recorded.metadata();
    assert_eq!(metadata.engine, "colima-nerdctl");
    // The shape Prepare writes (#230): engine, context and a source label.
    assert_eq!(metadata.source, "driver-default");
    let Observed::Verified(read, _) = BackendSelection::from_metadata(&metadata) else {
        panic!("recorded selection unreadable");
    };
    assert_eq!(read.kind, BackendKind::Colima);
    assert_eq!(read.context, "fixture");
    assert_eq!(
        read.reason,
        BackendReason::Recorded("driver-default".into())
    );
    let mut foreign = metadata.clone();
    foreign.engine = "podman".into();
    assert!(matches!(
        BackendSelection::from_metadata(&foreign),
        Observed::Unknown(_)
    ));
    let mut torn = metadata;
    torn.source = " ".into();
    assert!(matches!(
        BackendSelection::from_metadata(&torn),
        Observed::Unknown(_)
    ));
}

#[test]
fn a_bring_up_after_an_earlier_stop_activates() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    bring_up(
        &ctx,
        &stack,
        &fake,
        &closure("one"),
        Retention::Persistent,
        &[],
    )
    .unwrap();
    take_down(&ctx, &stack, Operation::Down, &fake, None).unwrap();
    assert_eq!(record(&ctx, &stack).state, EnvState::Stopped);
    // Only a stop during this bring-up's integrations refuses activation.
    assert_eq!(
        bring_up(
            &ctx,
            &stack,
            &fake,
            &closure("one"),
            Retention::Persistent,
            &[]
        )
        .unwrap(),
        Applied::Reconciled
    );
    assert!(record(&ctx, &stack).active());
    assert_eq!(fake.calls(), ["apply", "stop", "apply"]);
    assert_store_sound(&ctx);
}

/// The settlement budget is the settlement's own: a context opened long
/// before a settlement still hands that settlement the whole of it, and the
/// next settlement gets another (effigy#447).
#[test]
fn a_settlement_gets_the_full_budget_when_it_starts() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    // Activation time passes before settlement begins.
    std::thread::sleep(std::time::Duration::from_millis(50));
    for attempt in 0..2 {
        let remaining = ctx
            .cleanup()
            .unwrap()
            .operation()
            .remaining()
            .expect("a finite settlement budget");
        assert!(
            remaining >= CLEANUP_BUDGET - std::time::Duration::from_millis(50),
            "settlement {attempt} got {remaining:?} of {CLEANUP_BUDGET:?}"
        );
    }
}

/// effigy#447: settlement used to run on the cleanup context made when the
/// environment context opened, so an activation slower than that budget left
/// its settlement already expired and the record timed out. Each settlement
/// now starts its own bounded budget: the activation below spends all of the
/// budget it was given, and both of its runs still settle afterwards. On the
/// one-shot context this returned `Err` from the apply run's settle.
#[test]
fn a_bring_up_that_outlives_its_budget_still_settles() {
    let home = home();
    let root = root(home.path());
    // The settlement budget the activation spends entirely.
    let ctx = EnvContext::with_cleanup_budget(
        &root,
        CancelToken::default(),
        std::time::Duration::from_millis(700),
    )
    .unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let render = closure("one");
    let applied = reconcile(
        &ctx,
        &stack,
        Up {
            render: Some(&render),
            project: PROJECT,
            backend: &selection(),
            retention: Retention::Held,
            workdir: std::path::Path::new("."),
            primary_service: "app",
            compose_files: &[],
        },
        || {
            // A cold stack's bring-up, longer than the settlement budget.
            std::thread::sleep(std::time::Duration::from_millis(900));
            fake.apply()
        },
    )
    .unwrap()
    .applied;
    assert_eq!(applied, Applied::Reconciled);
    // The activation run that follows the slow apply also settles on a budget
    // of its own, and publishes `active`.
    std::thread::sleep(std::time::Duration::from_millis(900));
    activate(&ctx, &stack, &[]).unwrap();
    assert!(record(&ctx, &stack).active());
    assert_eq!(fake.calls(), ["apply"]);
    assert_store_sound(&ctx);
}

/// The boundary at production size: an activation that spends more than the
/// real 60-second settlement budget still settles, and the activation run that
/// follows it settles too. The shortened-budget proofs above show the ordering;
/// this one spends the production budget itself, the shape the I1 cold stack
/// hit when its first build ran past a minute.
#[test]
fn an_activation_past_the_sixty_second_budget_still_settles() {
    const ACTIVATION: std::time::Duration = std::time::Duration::from_secs(61);
    assert!(
        ACTIVATION > CLEANUP_BUDGET,
        "the activation must outlive the production settlement budget"
    );
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let render = closure("one");
    let started = std::time::Instant::now();
    assert_eq!(
        reconcile(
            &ctx,
            &stack,
            Up {
                render: Some(&render),
                project: PROJECT,
                backend: &selection(),
                retention: Retention::Held,
                workdir: std::path::Path::new("."),
                primary_service: "app",
                compose_files: &[],
            },
            || {
                std::thread::sleep(ACTIVATION);
                fake.apply()
            },
        )
        .unwrap()
        .applied,
        Applied::Reconciled
    );
    assert!(
        started.elapsed() > CLEANUP_BUDGET,
        "the activation did not outlive the budget: {:?}",
        started.elapsed()
    );
    // Its own settlement had a whole window, and the activation run that
    // follows publishes `active` from its own.
    activate(&ctx, &stack, &[]).unwrap();
    assert!(record(&ctx, &stack).active());
    assert_eq!(fake.calls(), ["apply"]);
    assert_store_sound(&ctx);
}

/// The operator sees which phase a transition is in: one live step per
/// transition names the gate it waits for and each reconcile phase, and the
/// run ends with its own settlement line, so a slow activation never looks
/// like a hang and an expired settle is named where it happened (effigy#461,
/// effigy#447). The frame text itself is proven by
/// `environment::reconcile::tests`; this pins the finish lines a piped run sees.
#[test]
fn a_transition_names_its_wait_and_its_settlement() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let (applied, lines) = effigy_ui::steps::capture_lines(|| {
        bring_up(&ctx, &stack, &fake, &closure("one"), Retention::Held, &[])
    });
    assert_eq!(applied.unwrap(), Applied::Reconciled);
    // Two runs, so the apply run and the activation each end with their own
    // settlement, and an admission contributes no separate line.
    assert_eq!(
        lines,
        vec![
            "✓ environment `stack` settled".to_owned(),
            "✓ environment `stack` settled".to_owned(),
        ]
    );
}

/// An activation stopped after its backend effect settles without dispatching
/// another one, and without an expired budget: the settle's context starts
/// when the settle starts (effigy#447).
#[test]
fn an_interrupted_activation_settles_without_another_effect() {
    let home = home();
    let root = root(home.path());
    let cancel = CancelToken::default();
    let ctx = EnvContext::with_cleanup_budget(
        &root,
        cancel.clone(),
        std::time::Duration::from_millis(700),
    )
    .unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast).unwrap();
    run.publish_desired(&closure("one")).unwrap();
    let mut backend = fake.clone();
    run.create_with(&mut backend, &selection(), create_request(), || {
        fake.apply()
    })
    .unwrap();
    // The operator stops the activation after the backend acted.
    cancel.cancel(effigy_kernel::ctx::StopCause::Signal(2));
    std::thread::sleep(std::time::Duration::from_millis(900));
    assert_eq!(run.finish(false).unwrap(), CleanupOutcome::Complete);
    assert_eq!(
        fake.calls(),
        ["apply"],
        "settlement dispatched a backend effect after the stop"
    );
    // The cancelled token stays on that invocation's reads; the record the
    // settle left behind is read from a fresh context.
    let settled = EnvContext::new(&root, CancelToken::default()).unwrap();
    assert!(!record(&settled, &stack).active());
    assert_store_sound(&settled);
}

/// A failed activation settles on its own budget too, so the record is left
/// settled and the next up reconciles it from what is running (effigy#447).
#[test]
fn a_failed_activation_settles_on_its_own_budget() {
    let home = home();
    let root = root(home.path());
    let ctx = EnvContext::with_cleanup_budget(
        &root,
        CancelToken::default(),
        std::time::Duration::from_millis(700),
    )
    .unwrap();
    let stack = env("stack");
    let fake = Fake::new();
    fake.queue(Err(BackendError::NotStarted("runtime refused".into())));
    let render = closure("one");
    let failed = reconcile(
        &ctx,
        &stack,
        Up {
            render: Some(&render),
            project: PROJECT,
            backend: &selection(),
            retention: Retention::Held,
            workdir: std::path::Path::new("."),
            primary_service: "app",
            compose_files: &[],
        },
        || {
            std::thread::sleep(std::time::Duration::from_millis(900));
            fake.apply()
        },
    );
    assert!(matches!(failed, Err(EnvError::Backend(_))), "{failed:?}");
    assert!(!record(&ctx, &stack).active());
    assert_store_sound(&ctx);
    // The run settled: the retry applies once, with no predecessor residue.
    bring_up(&ctx, &stack, &fake, &render, Retention::Held, &[]).unwrap();
    assert!(record(&ctx, &stack).active());
    assert_eq!(fake.calls(), ["apply"]);
    assert_store_sound(&ctx);
}

#[test]
fn checkout_state_routing_keeps_environment_holders_and_objects_local() {
    let machine = home();
    let checkout = home();
    crate::with_test_effigy_home(&machine.path().join(".effigy"), || {
        let op = reconcile::operation(EffectClass::Mutate, CancelToken::default(), RUN_BUDGET);
        let cx = op.effects().unwrap();
        let observe = op.observe().unwrap();
        assert!(matches!(
            super::observe_root(&observe, checkout.path()),
            Observed::Absent(_)
        ));
        assert!(!checkout.path().join(".effigy").exists());
        let root = super::state_root(&cx, checkout.path()).unwrap();
        let ctx = EnvContext::new(&root, CancelToken::default()).unwrap();
        let stack = env("local-routing");
        let mut task = holder(&ctx, "local-holder", 81);
        let fake = Fake::new();
        let render = closure("local");
        bring_up(
            &ctx,
            &stack,
            &fake,
            &render,
            Retention::Held,
            &[(task.id(), AttachMode::Borrow)],
        )
        .unwrap();
        let current = record(&ctx, &stack);
        assert!(current.active());
        assert!(
            matches!(current.holders, Observed::Verified(ref holders, _) if holders.iter().any(|holder| holder.run == task.id()))
        );
        let object = effigy_kernel::objects::Objects::resolve(
            &observe,
            &root,
            &stack.resource,
            effigy_kernel::objects::Role::Desired,
        );
        assert!(object
            .verified()
            .unwrap()
            .path
            .starts_with(checkout.path().join(".effigy/state/objects")));
        assert!(!machine.path().join(".effigy/state").exists());
        task.finish(WorkloadOutcome::Succeeded).unwrap();
        task.settle(&settlement(&ctx), &mut EnvSettlement).unwrap();
    });
}
