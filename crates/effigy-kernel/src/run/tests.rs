use super::*;
use crate::ctx::*;
use crate::id::*;
use crate::observe::*;
use crate::outcome::{CleanupOutcome, Warning};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) fn context() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap()
}
pub(super) fn cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation().cleanup(Duration::from_secs(10)).unwrap()
}
pub(super) fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], |args| args == ["recover"]).unwrap()
}
pub(super) fn id(n: u8) -> RunId {
    RunId::new(1, [n; 10]).unwrap()
}
pub(super) fn owner() -> ResourceId {
    TaskId::new(checkout(), "", Surface::Published, "task", None)
        .unwrap()
        .resource()
}
fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16])
}
fn env() -> ResourceId {
    ResourceId::environment(checkout(), "stack").unwrap()
}
pub(super) fn inv(owner: ResourceId) -> ResolvedInvocation {
    ResolvedInvocation::new(InvocationSpec {
        task: TaskId::new(checkout(), "", Surface::Published, "task", None).unwrap(),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap()
}
pub(super) fn opts(n: u8) -> BeginOpts {
    BeginOpts {
        id: id(n),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: None,
    }
}
fn begin<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    owner: ResourceId,
    n: u8,
    fake: &mut Fake,
) -> Run<'a> {
    Run::begin(cx, root, &inv(owner), opts(n), &cleanup(cx), fake).unwrap()
}
pub(super) fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("effigy-run-")
        .tempdir()
        .unwrap()
}

/// A strict external-effect adapter: durable objects and a call log, no
/// permissive fallback. A second application or release panics. Recovery must
/// observe absence and skip undo after a crash at AdapterAfter.
#[derive(Debug)]
pub(super) struct Fake {
    home: PathBuf,
    unknown: bool,
    fail_release: bool,
}
impl Fake {
    pub(super) fn new(home: &Path) -> Self {
        Self {
            home: home.into(),
            unknown: false,
            fail_release: false,
        }
    }
    fn path(&self, effect: &Effect) -> PathBuf {
        let key = effect.spec.payload.as_str().expect("fake payload key");
        assert!(!key.contains('/'));
        self.home.join(key)
    }
    fn log(&self, action: &str, key: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.home.join("calls"))
            .unwrap();
        writeln!(file, "{action} {key}").unwrap();
        file.sync_all().unwrap();
    }
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.home.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn apply(&self, run: &Run<'_>, ticket: &EffectTicket) {
        assert!(!crate::store::in_transaction());
        let effect = journal::effect(run.store.conn(), run.id, ticket.sequence).unwrap();
        assert_eq!(effect.state, EffectState::Intended);
        let path = self.path(&effect);
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap();
        file.write_all(
            serde_json::to_string(&effect.spec.target)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        file.sync_all().unwrap();
        self.log("apply", effect.spec.payload.as_str().unwrap());
    }
}
impl SettlementAdapter for Fake {
    fn observe(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<Fence> {
        assert!(!crate::store::in_transaction());
        if self.unknown {
            return Observed::Unknown(UnknownReason::Injected);
        }
        match std::fs::read(self.path(effect)) {
            Ok(bytes) => {
                Observed::Verified(serde_json::from_slice(&bytes).unwrap(), settle::evidence())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Observed::Absent(AbsenceProof::NoFile {
                    name: effect.spec.payload.as_str().unwrap().into(),
                })
            }
            Err(e) => panic!("unconfigured fake error: {e}"),
        }
    }
    fn release(&mut self, effect: &Effect, _: &CleanupContext) -> Observed<()> {
        assert!(!crate::store::in_transaction());
        assert!(!self.unknown);
        if self.fail_release {
            return Observed::Unknown(UnknownReason::Injected);
        }
        std::fs::remove_file(self.path(effect)).expect("exactly one undo");
        self.log("release", effect.spec.payload.as_str().unwrap());
        Observed::Verified((), settle::evidence())
    }
    fn reconcile(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<Reconciled> {
        self.release(effect, cx).map(|()| Reconciled::Released)
    }
}
fn spec(run: &Run<'_>, key: &str, settlement: Settlement) -> EffectSpec {
    EffectSpec {
        target: run.fence().clone(),
        kind: "fake".into(),
        payload: Value::String(key.into()),
        settlement,
        required: true,
    }
}
fn apply(run: &mut Run<'_>, fake: &Fake, key: &str) -> EffectTicket {
    let ticket = run
        .intend(spec(run, key, Settlement::Release(Value::Null)))
        .unwrap();
    fake.apply(run, &ticket);
    run.applied(&ticket, Value::Bool(true)).unwrap();
    run.completed(&ticket, crate::outcome::RequiredObligation::Applied)
        .unwrap();
    ticket
}
fn active_environment(cx: &EffectContext, root: &StateRoot, fake: &mut Fake) -> Fence {
    let mut run = begin(cx, root, env(), 90, fake);
    Attachments::publish_active(&mut run).unwrap();
    let fence = run.fence().clone();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cleanup(cx), fake).unwrap();
    fence
}
fn reader(cx: &EffectContext, root: &StateRoot) -> crate::store::StoreReader {
    match Store::read(&cx.operation().observe().unwrap(), root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("{other:?}"),
    }
}

#[test]
fn run_reverse_settle_is_idempotent_and_preserves_workload_and_observability() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    apply(&mut run, &fake, "owned-a");
    apply(&mut run, &fake, "owned-b");
    run.finish(WorkloadOutcome::Failed { code: 7 }).unwrap();
    let mut outcome = run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert_eq!(outcome.exit_code(), 7);
    outcome.add_warning(Warning {
        code: "ENOSPC".into(),
        message: "status write failed".into(),
    });
    assert_eq!(outcome.exit_code(), 7);
    assert_eq!(
        fake.calls(),
        [
            "apply owned-a",
            "apply owned-b",
            "release owned-b",
            "release owned-a"
        ]
    );
    let retry = settle_interrupted(
        &cx,
        &root,
        &owner(),
        id(1),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert_eq!(retry.workload(), outcome.workload());
    assert_eq!(retry.cleanup(), outcome.cleanup());
    assert_eq!(fake.calls().len(), 4);
}
#[test]
fn run_intended_absent_present_unknown_and_superseded() {
    for mode in ["absent", "present", "unknown", "superseded"] {
        let home = home();
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut fake = Fake::new(home.path());
        let mut run = begin(&cx, &root, owner(), 1, &mut fake);
        let ticket = run
            .intend(spec(&run, "owned", Settlement::Release(Value::Null)))
            .unwrap();
        if mode != "absent" {
            fake.apply(&run, &ticket);
        }
        if mode == "superseded" {
            std::fs::write(
                home.path().join("owned"),
                serde_json::to_string(&Fence {
                    resource: owner(),
                    generation: Generation::new(99),
                })
                .unwrap(),
            )
            .unwrap();
        }
        fake.unknown = mode == "unknown";
        drop(run);
        let result = settle_interrupted(
            &cx,
            &root,
            &owner(),
            id(1),
            next(),
            &cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        assert_eq!(result.workload(), &WorkloadOutcome::Interrupted);
        assert_eq!(
            result.cleanup(),
            if mode == "unknown" {
                CleanupOutcome::Incomplete
            } else {
                CleanupOutcome::Complete
            }
        );
        assert_eq!(
            home.path().join("owned").exists(),
            matches!(mode, "unknown" | "superseded")
        );
    }
}
#[test]
fn run_irreversible_retained_and_failed_effects_never_replay() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    for (key, settlement) in [
        ("retain", Settlement::Retain),
        ("irreversible", Settlement::Irreversible),
    ] {
        let ticket = run.intend(spec(&run, key, settlement)).unwrap();
        fake.apply(&run, &ticket);
        run.applied(&ticket, Value::Null).unwrap();
    }
    let ticket = run
        .intend(spec(&run, "failed", Settlement::Release(Value::Null)))
        .unwrap();
    run.failed(
        &ticket,
        AbsenceProof::NoFile {
            name: "failed".into(),
        },
    )
    .unwrap();
    assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
    run.finish(WorkloadOutcome::Failed { code: 2 }).unwrap();
    let result = run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    assert_eq!(fake.calls(), ["apply retain", "apply irreversible"]);
}
#[test]
fn run_closed_residue_blocks_until_explicit_ack_and_stays_retryable() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    apply(&mut run, &fake, "owned");
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    fake.unknown = true;
    let result = run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Incomplete);
    assert_ne!(result.exit_code(), 0);
    assert!(
        matches!(Run::begin(&cx,&root,&inv(owner()),opts(2),&cleanup(&cx),&mut fake),Err(RunError::PredecessorUnsettled { run,.. }) if run==id(1))
    );
    let mut acknowledged = opts(2);
    acknowledged.acknowledge.push(id(1));
    let run = Run::begin(
        &cx,
        &root,
        &inv(owner()),
        acknowledged,
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert_eq!(run.fence().generation, Generation::new(2));
    assert!(home.path().join("owned").exists());
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    fake.unknown = false;
    let run = begin(&cx, &root, owner(), 3, &mut fake);
    assert!(!home.path().join("owned").exists());
    let previous = journal::run(run.store.conn(), id(1)).unwrap();
    assert_eq!(previous.workload, Some(WorkloadOutcome::Succeeded));
    assert!(previous.acknowledged);
    run.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn run_interrupted_predecessor_settles_before_admission_and_stale_requests_refuse() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    let old = run.fence().clone();
    apply(&mut run, &fake, "owned");
    drop(run);
    let observation = cx.operation().observe().unwrap();
    let view = reader(&cx, &root);
    assert!(matches!(
        status(&observation, &root, &view, id(1), next()),
        Observed::Verified(RunStatus::Interrupted { .. }, _)
    ));
    let run = begin(&cx, &root, owner(), 2, &mut fake);
    assert!(!home.path().join("owned").exists());
    assert!(matches!(
        request(
            &cx,
            &root,
            &Request {
                id: id(40),
                run: id(1),
                expected: old,
                action: RequestAction::Stop
            }
        ),
        Err(RunError::StaleGeneration { .. })
    ));
    request(
        &cx,
        &root,
        &Request {
            id: id(41),
            run: id(2),
            expected: run.fence().clone(),
            action: RequestAction::Stop,
        },
    )
    .unwrap();
    assert_eq!(run.requests().unwrap().len(), 1);
    run.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn run_callbacks_and_effects_refuse_stale_generation_and_new_effects_after_stopping() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    let mut stale = spec(&run, "stale", Settlement::Retain);
    stale.target.generation = Generation::new(0);
    assert!(matches!(
        run.intend(stale),
        Err(RunError::StaleGeneration { .. })
    ));
    drop(run);
    let mut run = begin(&cx, &root, owner(), 2, &mut fake);
    let ticket = run
        .intend(spec(&run, "owned", Settlement::Release(Value::Null)))
        .unwrap();
    // Strictly simulate a delayed callback with the wrong owner generation.
    let mut stale = ticket.clone();
    stale.owner.generation = Generation::new(1);
    assert!(run.applied(&stale, Value::Null).is_err());
    run.advance(Phase::Stopping).unwrap();
    assert!(run.intend(spec(&run, "late", Settlement::Retain)).is_err());
    run.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn run_cleanup_uses_independent_token_and_expired_cleanup_never_undoes() {
    for expired in [false, true] {
        let home = home();
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut fake = Fake::new(home.path());
        let mut run = begin(&cx, &root, owner(), 1, &mut fake);
        apply(&mut run, &fake, "owned");
        cx.operation().cancel_token().cancel(StopCause::Requested);
        let cleanup = cx
            .operation()
            .cleanup(if expired {
                Duration::ZERO
            } else {
                Duration::from_secs(3)
            })
            .unwrap();
        let result = run.settle(&cleanup, &mut fake);
        if expired {
            assert!(result.is_err());
            assert!(home.path().join("owned").exists());
        } else {
            assert_eq!(result.unwrap().cleanup(), CleanupOutcome::Complete);
            assert!(!home.path().join("owned").exists());
        }
    }
}
#[test]
fn attachments_live_interrupted_and_unknown_holders_block_every_transition() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let env_fence = active_environment(&cx, &root, &mut fake);
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    Attachments::attach(&mut holder, &env_fence, AttachMode::Borrow, Wait::FailFast).unwrap();
    let mut transition = begin(&cx, &root, env(), 91, &mut fake);
    for _policy in ["down", "failed recovery", "idle stop", "handover"] {
        assert!(matches!(
            Attachments::permit(&mut transition),
            Err(RunError::HeldBy(_))
        ));
    }
    let view = reader(&cx, &root);
    let observed = cx.operation().observe().unwrap();
    assert!(
        matches!(Attachments::holders(&observed,&root,&view,&env()),Observed::Verified(ref holders,_) if matches!(holders[0].liveness,Observed::Verified(Liveness::Live,_)))
    );
    drop(holder);
    assert!(
        matches!(Attachments::holders(&observed,&root,&view,&env()),Observed::Verified(ref holders,_) if matches!(holders[0].liveness,Observed::Verified(Liveness::Interrupted,_)))
    );
    assert!(Attachments::permit(&mut transition).is_err());
    // Holder settlement requires no environment gate and completes while the
    // transition keeps that gate. Exactly one attachment release is durable.
    settle_interrupted(
        &cx,
        &root,
        &owner(),
        id(1),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert!(Attachments::permit(&mut transition).is_ok());
    assert!(fake.calls().is_empty());
    transition.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn attachments_residue_keeps_holder_and_claim_protection_even_when_acknowledged() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let env_fence = active_environment(&cx, &root, &mut fake);
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    Attachments::attach(&mut holder, &env_fence, AttachMode::Borrow, Wait::FailFast).unwrap();
    let data = ResourceId::data("fake", "local", "volume").unwrap();
    Claims::reserve(&mut holder, std::slice::from_ref(&data), Duration::ZERO).unwrap();
    apply(&mut holder, &fake, "owned");
    fake.unknown = true;
    assert_eq!(
        holder.settle(&cleanup(&cx), &mut fake).unwrap().cleanup(),
        CleanupOutcome::Incomplete
    );
    let view = reader(&cx, &root);
    let observed = cx.operation().observe().unwrap();
    assert!(
        matches!(Attachments::holders(&observed,&root,&view,&env()),Observed::Verified(ref holders,_) if matches!(holders[0].liveness,Observed::Unknown(_)))
    );
    let mut transition = begin(&cx, &root, env(), 91, &mut fake);
    assert!(Attachments::permit(&mut transition).is_err());
    let mut other = begin(
        &cx,
        &root,
        ResourceId::gateway("other").unwrap(),
        3,
        &mut fake,
    );
    assert!(matches!(
        Claims::reserve(&mut other, &[data], Duration::ZERO),
        Err(RunError::Conflict(_))
    ));
    drop(other);
    let mut acknowledged = opts(2);
    acknowledged.acknowledge.push(id(1));
    let successor = Run::begin(
        &cx,
        &root,
        &inv(owner()),
        acknowledged,
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert!(Attachments::permit(&mut transition).is_err());
    drop(successor);
    fake.unknown = false;
    settle_interrupted(
        &cx,
        &root,
        &owner(),
        id(1),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert!(Attachments::permit(&mut transition).is_ok());
    transition.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn attachments_stalled_permit_excludes_concurrent_attach_and_intent_rechecks() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let initial = active_environment(&cx, &root, &mut fake);
    let mut transition = begin(&cx, &root, env(), 91, &mut fake);
    let current = transition.fence().clone();
    let mut permit = Attachments::permit(&mut transition).unwrap();
    std::thread::scope(|scope| {
        let join = scope.spawn(|| {
            let cx = cx.clone();
            let root = StateRoot::effects(&cx, home.path()).unwrap();
            let mut fake = Fake::new(home.path());
            let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
            assert!(matches!(
                Attachments::attach(&mut holder, &initial, AttachMode::Borrow, Wait::FailFast),
                Err(RunError::Gate(GateError::Busy { .. }))
            ));
            holder.settle(&cleanup(&cx), &mut fake).unwrap();
        });
        join.join().unwrap();
        let spec = EffectSpec {
            target: current,
            kind: "fake".into(),
            payload: Value::String("transition".into()),
            settlement: Settlement::Retain,
            required: true,
        };
        let ticket = permit.intend(spec).unwrap();
        // The fake destructive action sees no attachment, even after a stalled permit.
        let view = reader(&cx, &root);
        assert!(
            matches!(Attachments::holders(&cx.operation().observe().unwrap(),&root,&view,&env()),Observed::Verified(ref holders,_) if holders.is_empty())
        );
        permit.applied(&ticket, Value::Bool(true)).unwrap();
        permit
            .completed(&ticket, crate::outcome::RequiredObligation::Applied)
            .unwrap();
    });
    transition.finish(WorkloadOutcome::Succeeded).unwrap();
    transition.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn claims_conflict_set_is_atomic_and_concurrent_starts_have_one_winner() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let data = ResourceId::data("fake", "local", "volume").unwrap();
    let port = ResourceId::endpoint(EndpointKind::Port, "8080").unwrap();
    let mut first = begin(&cx, &root, owner(), 1, &mut fake);
    let token = Claims::reserve(&mut first, std::slice::from_ref(&data), Duration::ZERO).unwrap();
    Claims::commit(&mut first, &token, 1).unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let cx = cx.clone();
                let root = StateRoot::effects(&cx, home.path()).unwrap();
                let mut fake = Fake::new(home.path());
                let mut other = begin(
                    &cx,
                    &root,
                    ResourceId::gateway("hybrid").unwrap(),
                    2,
                    &mut fake,
                );
                assert!(matches!(
                    Claims::reserve(&mut other, &[port.clone(), data.clone()], Duration::ZERO),
                    Err(RunError::Conflict(_))
                ));
                let count: i64 = other
                    .store
                    .conn()
                    .query_row(
                        "SELECT count(*) FROM claims WHERE resource=?1",
                        [port.name().to_string()],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(count, 0);
                other.settle(&cleanup(&cx), &mut fake).unwrap();
            })
            .join()
            .unwrap();
    });
    first.finish(WorkloadOutcome::Succeeded).unwrap();
    first.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn claims_quarantine_blocks_reuse_until_boot_deadline_and_release_is_once() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let address = ResourceId::endpoint(EndpointKind::Address, "127.0.0.9").unwrap();
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    Claims::reserve(
        &mut run,
        std::slice::from_ref(&address),
        Duration::from_secs(60),
    )
    .unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    let store = Store::open(&cx, &root).unwrap();
    let deadline: i64 = store
        .conn()
        .query_row("SELECT quarantine_nanos FROM claims", [], |r| r.get(0))
        .unwrap();
    settle_interrupted(
        &cx,
        &root,
        &owner(),
        id(1),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert_eq!(
        store
            .conn()
            .query_row("SELECT quarantine_nanos FROM claims", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        deadline
    );
    let mut other = begin(&cx, &root, owner(), 2, &mut fake);
    assert!(matches!(
        Claims::reserve(&mut other, std::slice::from_ref(&address), Duration::ZERO),
        Err(RunError::Conflict(_))
    ));
    drop(other);
    // Strict clock fixture: a different boot ends quarantine; no wall timestamp.
    store
        .conn()
        .execute("UPDATE claims SET quarantine_boot='previous-boot'", [])
        .unwrap();
    let mut next = begin(&cx, &root, owner(), 3, &mut fake);
    Claims::reserve(&mut next, &[address], Duration::ZERO).unwrap();
    next.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn run_concurrent_settle_has_one_owner_and_no_double_undo() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner(), 1, &mut fake);
    apply(&mut run, &fake, "owned");
    assert!(matches!(
        settle_interrupted(
            &cx,
            &root,
            &owner(),
            id(1),
            next(),
            &cleanup(&cx),
            &mut fake
        ),
        Err(RunError::Gate(GateError::Busy { .. }))
    ));
    drop(run);
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let mut handles = vec![];
        for _ in 0..2 {
            let barrier = &barrier;
            let home = &home;
            handles.push(scope.spawn(move || {
                let cx = context();
                let root = StateRoot::effects(&cx, home.path()).unwrap();
                let mut fake = Fake::new(home.path());
                barrier.wait();
                match settle_interrupted(
                    &cx,
                    &root,
                    &owner(),
                    id(1),
                    next(),
                    &cleanup(&cx),
                    &mut fake,
                ) {
                    Ok(result) => assert_eq!(result.cleanup(), CleanupOutcome::Complete),
                    Err(RunError::Gate(GateError::Busy { .. })) => {}
                    other => panic!("{other:?}"),
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    });
    assert_eq!(fake.calls(), ["apply owned", "release owned"]);
}
#[test]
fn run_identity_recovery_roundtrips_and_rejects_noncanonical_records() {
    for resource in [
        owner(),
        env(),
        id(1).resource(),
        checkout().resource(),
        ResourceId::gateway("main").unwrap(),
        ResourceId::store(Scope::Machine, "cache").unwrap(),
    ] {
        let encoded = serde_json::to_string(&resource).unwrap();
        assert_eq!(
            serde_json::from_str::<ResourceId>(&encoded).unwrap(),
            resource
        );
    }
    assert_eq!(id(1).to_string().parse::<RunId>().unwrap(), id(1));
    assert!("z".repeat(26).parse::<RunId>().is_err());
    assert!(serde_json::from_value::<ResourceId>(
        serde_json::json!({"kind":"Env","scope":"Machine","key":["test"]})
    )
    .is_err());
}

#[cfg(feature = "fault-injection")]
mod faults;

#[test]
fn run_applied_spawn_is_not_natural_completion() {
    for code in [0, 7] {
        let home = home();
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut fake = Fake::new(home.path());
        let mut run = begin(&cx, &root, owner(), 1, &mut fake);
        let ticket = run
            .intend(spec(&run, "owned", Settlement::Release(Value::Null)))
            .unwrap();
        fake.apply(&run, &ticket);
        run.applied(&ticket, Value::Bool(true)).unwrap();
        assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
        run.completed(
            &ticket,
            crate::outcome::RequiredObligation::ProcessExit(code),
        )
        .unwrap();
        if code == 0 {
            run.finish(WorkloadOutcome::Succeeded).unwrap();
        } else {
            assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
            run.finish(WorkloadOutcome::Failed { code }).unwrap();
        }
        run.settle(&cleanup(&cx), &mut fake).unwrap();
    }
}
#[test]
fn claims_simultaneous_reservations_commit_exactly_one_conflict_set() {
    let home = home();
    let barrier = std::sync::Barrier::new(2);
    let wins = std::thread::scope(|scope| {
        let handles = (1..=2)
            .map(|n| {
                let home = &home;
                let barrier = &barrier;
                scope.spawn(move || {
                    let cx = context();
                    let root = StateRoot::effects(&cx, home.path()).unwrap();
                    let mut fake = Fake::new(home.path());
                    let resource = ResourceId::gateway(&format!("start-{n}")).unwrap();
                    let mut run = begin(&cx, &root, resource, n, &mut fake);
                    let data = ResourceId::data("fake", "local", "shared").unwrap();
                    barrier.wait();
                    let won = match Claims::reserve(&mut run, &[data], Duration::ZERO) {
                        Ok(token) => {
                            Claims::commit(&mut run, &token, 1).unwrap();
                            true
                        }
                        Err(RunError::Conflict(_)) => false,
                        other => panic!("{other:?}"),
                    };
                    barrier.wait(); // Keep the winner's reservation until both attempted.
                    run.settle(&cleanup(&cx), &mut fake).unwrap();
                    won
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|h| usize::from(h.join().unwrap()))
            .sum::<usize>()
    });
    assert_eq!(wins, 1);
}

#[test]
fn claims_prepare_and_refresh_classes_cannot_reserve_resources() {
    for class in [EffectClass::Prepare, EffectClass::RefreshDerived] {
        let home = home();
        let original = context();
        let op = OperationContext::new(
            original.operation().deadline(),
            CancelToken::default(),
            original.operation().limits().clone(),
            class,
            Placement::Host,
        );
        let cx = op.effects().unwrap();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut fake = Fake::new(home.path());
        let mut spec = inv(ResourceId::store(Scope::Machine, "source").unwrap())
            .spec()
            .clone();
        spec.class = class;
        let mut run = Run::begin(
            &cx,
            &root,
            &ResolvedInvocation::new(spec).unwrap(),
            opts(1),
            &cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        assert!(matches!(
            Claims::reserve(
                &mut run,
                &[ResourceId::data("fake", "local", "data").unwrap()],
                Duration::ZERO
            ),
            Err(RunError::Invalid(_))
        ));
        let claims: i64 = run
            .store
            .conn()
            .query_row("SELECT count(*) FROM claims", [], |r| r.get(0))
            .unwrap();
        assert_eq!(claims, 0);
        run.settle(&cleanup(&cx), &mut fake).unwrap();
    }
}

#[test]
fn run_supervision_matrix_recovery_stop_and_attachment_release_are_idempotent() {
    for front in ["tui", "attached", "detached"] {
        for prior in ["fresh", "interrupted", "residue", "stopped"] {
            for stop in [
                "request",
                "INT",
                "TERM",
                "HUP",
                "client-kill",
                "owner-kill",
                "handover",
                "down",
            ] {
                for attachment in [
                    None,
                    Some(AttachMode::Borrow),
                    Some(AttachMode::Exclusive {
                        stop_on_release: true,
                    }),
                ] {
                    let home = home();
                    let cx = context();
                    let root = StateRoot::effects(&cx, home.path()).unwrap();
                    let mut fake = Fake::new(home.path());
                    if prior != "fresh" {
                        let mut previous = begin(&cx, &root, owner(), 1, &mut fake);
                        apply(&mut previous, &fake, "previous");
                        if prior == "interrupted" {
                            drop(previous);
                        } else {
                            fake.fail_release = prior == "residue";
                            previous.settle(&cleanup(&cx), &mut fake).unwrap();
                            if prior == "residue" {
                                assert!(
                                    matches!(
                                        Run::begin(
                                            &cx,
                                            &root,
                                            &inv(owner()),
                                            opts(2),
                                            &cleanup(&cx),
                                            &mut fake
                                        ),
                                        Err(RunError::PredecessorUnsettled { .. })
                                    ),
                                    "{front}/{prior}/{stop}"
                                );
                            }
                            fake.fail_release = false;
                        }
                    }
                    let environment = active_environment(&cx, &root, &mut fake);
                    let mut run = begin(&cx, &root, owner(), 2, &mut fake);
                    if let Some(mode) = attachment {
                        Attachments::attach(&mut run, &environment, mode, Wait::FailFast).unwrap();
                    }
                    apply(&mut run, &fake, "current");
                    match stop {
                        "owner-kill" => drop(run),
                        _ => {
                            let cause = match stop {
                                "INT" => StopCause::Signal(libc::SIGINT),
                                "TERM" => StopCause::Signal(libc::SIGTERM),
                                "HUP" => StopCause::Signal(libc::SIGHUP),
                                "client-kill" => StopCause::OwnerLost,
                                _ => StopCause::Requested,
                            };
                            let signal = match cause {
                                StopCause::Signal(s) => Some(s),
                                _ => None,
                            };
                            run.interrupt(
                                &cleanup(&cx),
                                WorkloadOutcome::Cancelled { cause, signal },
                            )
                            .unwrap();
                            run.settle(&cleanup(&cx), &mut fake).unwrap();
                        }
                    }
                    for _ in 0..2 {
                        assert_eq!(
                            settle_interrupted(
                                &cx,
                                &root,
                                &owner(),
                                id(2),
                                next(),
                                &cleanup(&cx),
                                &mut fake
                            )
                            .unwrap()
                            .cleanup(),
                            CleanupOutcome::Complete,
                            "{front}/{prior}/{stop}"
                        );
                    }
                    assert_eq!(
                        fake.calls()
                            .iter()
                            .filter(|s| s.as_str() == "release current")
                            .count(),
                        1
                    );
                    let released: i64 = Store::open(&cx, &root)
                        .unwrap()
                        .conn()
                        .query_row(
                            "SELECT count(*) FROM attachments WHERE run_id=?1 AND state='released'",
                            [id(2).to_string()],
                            |r| r.get(0),
                        )
                        .unwrap();
                    assert_eq!(
                        released,
                        i64::from(attachment.is_some()),
                        "{front}/{prior}/{stop}"
                    );
                }
            }
        }
    }
}

#[test]
fn run_store_record_uses_owner_gate_and_atomic_replace_faults() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let owner = ResourceId::store(Scope::Machine, "bundles").unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, owner, 1, &mut fake);
    let dir = crate::fs::DirFd::open(home.path()).unwrap();
    let name = crate::fs::FileName::new("sources.lock.json").unwrap();
    let ticket = run
        .intend(EffectSpec {
            target: run.fence().clone(),
            kind: "source-lock".into(),
            payload: Value::Null,
            settlement: Settlement::Retain,
            required: false,
        })
        .unwrap();
    run.replace_record(
        &dir,
        &name,
        b"previous",
        crate::record::ReplaceOpts::default(),
    )
    .unwrap();
    run.applied(&ticket, Value::Null).unwrap();
    #[cfg(feature = "fault-injection")]
    {
        crate::fault::arm_point(
            crate::fault::Point::ReplaceRename,
            crate::fault::Fault::Enospc,
        );
        assert!(run
            .replace_record(&dir, &name, b"next", crate::record::ReplaceOpts::default())
            .is_err());
        crate::fault::arm(None);
        assert_eq!(
            std::fs::read(home.path().join("sources.lock.json")).unwrap(),
            b"previous"
        );
    }
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(
        run.settle(&cleanup(&cx), &mut fake).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
}

#[test]
fn gateway_control_socket_lives_in_trusted_runtime_and_settles_exact_inode() {
    let home = tempfile::Builder::new()
        .prefix("g219-")
        .tempdir_in("/tmp")
        .unwrap();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let resource = ResourceId::gateway("operator").unwrap();
    let mut run = begin(&cx, &root, resource.clone(), 61, &mut fake);
    let listener = run.control_socket().unwrap();
    let path = control_socket_path(&root, &resource, run.id());
    assert!(path.starts_with(
        std::fs::canonicalize(home.path())
            .unwrap()
            .join(".effigy/gateway/runtime")
    ));
    assert!(std::os::unix::net::UnixStream::connect(&path).is_ok());
    drop(listener);
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert!(!path.exists());
}
#[test]
fn gateway_setup_replace_is_journalled_and_preserved_on_recovery() {
    use crate::fs::{DirFd, FileName};
    use crate::record::ReplaceOpts;
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let resource = ResourceId::store(Scope::Machine, "gateway-setup").unwrap();
    let mut run = begin(&cx, &root, resource.clone(), 62, &mut fake);
    let dir = DirFd::open(home.path()).unwrap();
    run.replace_file(
        &dir,
        &FileName::new("setup-output").unwrap(),
        b"owned platform output",
        ReplaceOpts::default(),
    )
    .unwrap();
    assert_eq!(run.effects().unwrap().len(), 1);
    drop(run);
    settle_interrupted(
        &cx,
        &root,
        &resource,
        id(62),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(home.path().join("setup-output")).unwrap(),
        b"owned platform output"
    );
}
#[test]
fn gateway_store_latest_run_tracks_generation_and_absence() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let resource = ResourceId::gateway("operator").unwrap();
    let run = begin(&cx, &root, resource.clone(), 63, &mut fake);
    let observe = cx.operation().observe().unwrap();
    let reader = match Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("{other:?}"),
    };
    assert_eq!(reader.latest_run(&resource).verified().unwrap().0, id(63));
    assert!(matches!(
        reader.latest_run(&ResourceId::gateway("missing").unwrap()),
        Observed::Absent(_)
    ));
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    let run = begin(&cx, &root, resource.clone(), 64, &mut fake);
    assert_eq!(reader.latest_run(&resource).verified().unwrap().0, id(64));
    run.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn environment_record_is_fenced_bounded_and_read_without_migration() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = begin(&cx, &root, env(), 90, &mut fake);
    let record = serde_json::json!({"retention": "persistent"});
    run.set_engine_record(&record).unwrap();
    let oversized = Value::String("x".repeat(20 << 10));
    assert!(matches!(
        run.set_engine_record(&oversized),
        Err(RunError::Store(StoreError::TooLarge { .. }))
    ));
    // A size refusal writes nothing and leaves the run usable.
    run.set_engine_record(&record).unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    let view = reader(&cx, &root);
    match view.resource(&env()) {
        Observed::Verified(resource, _) => assert_eq!(resource.engine, Some(record)),
        other => panic!("{other:?}"),
    }
    assert!(matches!(view.object_refs(&env()), Observed::Verified(refs, _) if refs.is_empty()));
    // A run writes only its own owner's record: a task run never reaches
    // the environment row.
    let mut stale = begin(&cx, &root, owner(), 1, &mut fake);
    stale
        .set_engine_record(&serde_json::json!({"foreign": true}))
        .unwrap();
    stale.settle(&cleanup(&cx), &mut fake).unwrap();
    match reader(&cx, &root).resource(&env()) {
        Observed::Verified(resource, _) => {
            assert_eq!(
                resource.engine,
                Some(serde_json::json!({"retention": "persistent"}))
            )
        }
        other => panic!("{other:?}"),
    }
}
#[test]
fn environment_stopped_publication_needs_a_permit_and_blocks_attach() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let active = active_environment(&cx, &root, &mut fake);
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    Attachments::attach(&mut holder, &active, AttachMode::Borrow, Wait::FailFast).unwrap();
    let mut down = begin(&cx, &root, env(), 91, &mut fake);
    assert!(matches!(
        Attachments::permit(&mut down),
        Err(RunError::HeldBy(_))
    ));
    down.settle(&cleanup(&cx), &mut fake).unwrap();
    holder.settle(&cleanup(&cx), &mut fake).unwrap();
    let mut down = begin(&cx, &root, env(), 92, &mut fake);
    let stopped = down.fence().clone();
    Attachments::permit(&mut down)
        .unwrap()
        .publish_stopped()
        .unwrap();
    down.finish(WorkloadOutcome::Succeeded).unwrap();
    down.settle(&cleanup(&cx), &mut fake).unwrap();
    match reader(&cx, &root).resource(&env()) {
        Observed::Verified(resource, _) => assert_eq!(resource.state.as_deref(), Some("stopped")),
        other => panic!("{other:?}"),
    }
    let mut late = begin(&cx, &root, owner(), 2, &mut fake);
    assert!(matches!(
        Attachments::attach(&mut late, &stopped, AttachMode::Borrow, Wait::FailFast),
        Err(RunError::EnvNotActive)
    ));
    late.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn environment_apply_run_attaches_requesting_holder_in_its_final_transaction() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    let mut apply = begin(&cx, &root, env(), 90, &mut fake);
    let mode = AttachMode::Exclusive {
        stop_on_release: true,
    };
    Attachments::publish_active_for(&mut apply, &[(holder.id(), mode)]).unwrap();
    // Idempotent for a holder already attached.
    Attachments::publish_active_for(&mut apply, &[(holder.id(), mode)]).unwrap();
    apply.finish(WorkloadOutcome::Succeeded).unwrap();
    apply.settle(&cleanup(&cx), &mut fake).unwrap();
    let view = reader(&cx, &root);
    let observed = cx.operation().observe().unwrap();
    assert!(
        matches!(Attachments::holders(&observed,&root,&view,&env()),Observed::Verified(ref holders,_) if holders.len() == 1 && holders[0].run == holder.id())
    );
    assert_eq!(
        holder
            .effects()
            .unwrap()
            .iter()
            .filter(|e| e.state == EffectState::Applied)
            .count(),
        1
    );
    let mut down = begin(&cx, &root, env(), 91, &mut fake);
    assert!(matches!(
        Attachments::permit(&mut down),
        Err(RunError::HeldBy(_))
    ));
    holder.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(
        holder.settle(&cleanup(&cx), &mut fake).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
    assert!(Attachments::permit(&mut down).is_ok());
    down.settle(&cleanup(&cx), &mut fake).unwrap();
    // A settling run can no longer be attached on its behalf.
    let mut late = begin(&cx, &root, owner(), 2, &mut fake);
    late.finish(WorkloadOutcome::Succeeded).unwrap();
    let mut apply = begin(&cx, &root, env(), 92, &mut fake);
    assert!(Attachments::publish_active_for(&mut apply, &[(late.id(), mode)]).is_err());
    apply.settle(&cleanup(&cx), &mut fake).unwrap();
    late.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn environment_exempt_permit_admits_only_the_settling_holder_own_stop() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let active = active_environment(&cx, &root, &mut fake);
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    Attachments::attach(&mut holder, &active, AttachMode::Borrow, Wait::FailFast).unwrap();
    let mut other = begin(
        &cx,
        &root,
        TaskId::new(checkout(), "", Surface::Published, "other", None)
            .unwrap()
            .resource(),
        2,
        &mut fake,
    );
    Attachments::attach(&mut other, &active, AttachMode::Borrow, Wait::FailFast).unwrap();
    let mut down = begin(&cx, &root, env(), 91, &mut fake);
    // A live, not-yet-settling holder is never exempt.
    assert!(matches!(
        Attachments::permit_exempt(&mut down, holder.id()),
        Err(RunError::HeldBy(_))
    ));
    holder.finish(WorkloadOutcome::Succeeded).unwrap();
    // Another live holder still protects the environment from the own stop.
    assert!(matches!(
        Attachments::permit_exempt(&mut down, holder.id()),
        Err(RunError::HeldBy(_))
    ));
    other.finish(WorkloadOutcome::Succeeded).unwrap();
    other.settle(&cleanup(&cx), &mut fake).unwrap();
    let mut permit = Attachments::permit_exempt(&mut down, holder.id()).unwrap();
    let ticket = permit
        .intend(EffectSpec {
            target: permit.fence().clone(),
            kind: "fake".into(),
            payload: Value::String("stop".into()),
            settlement: Settlement::Irreversible,
            required: true,
        })
        .unwrap();
    permit.applied(&ticket, Value::Bool(true)).unwrap();
    permit
        .completed(&ticket, crate::outcome::RequiredObligation::Applied)
        .unwrap();
    permit.publish_stopped().unwrap();
    down.finish(WorkloadOutcome::Succeeded).unwrap();
    // The holder still has its attachment while the stop records its
    // irreversible effect: recording calls no adapter, so cleanup completes.
    assert_eq!(
        down.settle(&cleanup(&cx), &mut fake).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
    assert!(fake.calls().is_empty());
    assert_eq!(
        holder.settle(&cleanup(&cx), &mut fake).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
}

#[test]
fn run_children_recover_and_require_positive_completion() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut parent = begin(&cx, &root, owner(), 220, &mut fake);
    assert!(parent
        .begin_child(&inv(owner()), opts(221), &cleanup(&cx), &mut fake)
        .is_err());
    let child_owner = ResourceId::store(Scope::Checkout(checkout()), "child").unwrap();
    let mut child = parent
        .begin_child(&inv(child_owner), opts(221), &cleanup(&cx), &mut fake)
        .unwrap();
    assert!(parent.finish(WorkloadOutcome::Succeeded).is_err());
    child.finish(WorkloadOutcome::Succeeded).unwrap();
    child.settle(&cleanup(&cx), &mut fake).unwrap();
    parent.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(
        parent.settle(&cleanup(&cx), &mut fake).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
    let reader = reader(&cx, &root);
    let rows = children(&reader, id(220));
    assert_eq!(rows.verified().unwrap()[0].parent, Some(id(220)));
}

#[test]
fn run_children_interruption_cascades_through_the_one_settle() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut parent = begin(&cx, &root, owner(), 222, &mut fake);
    let child_owner = ResourceId::store(Scope::Checkout(checkout()), "child").unwrap();
    let mut child = parent
        .begin_child(
            &inv(child_owner.clone()),
            opts(223),
            &cleanup(&cx),
            &mut fake,
        )
        .unwrap();
    let ticket = child
        .intend(EffectSpec {
            target: child.fence().clone(),
            kind: "fixture".into(),
            payload: Value::Null,
            settlement: Settlement::Retain,
            required: false,
        })
        .unwrap();
    child.applied(&ticket, Value::Null).unwrap();
    drop(child);
    drop(parent);
    let outcome = settle_interrupted(
        &cx,
        &root,
        &owner(),
        id(222),
        next(),
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    let reader = Store::read(&cx.operation().observe().unwrap(), &root);
    let rows = history(reader.verified().unwrap(), &child_owner, 1);
    assert_eq!(
        rows.verified().unwrap()[0].workload,
        Some(WorkloadOutcome::Interrupted)
    );
    assert_eq!(rows.verified().unwrap()[0].cleanup_complete, Some(true));
}

#[cfg(feature = "fault-injection")]
#[test]
fn run_children_admission_faults_recover_both_sides() {
    for point in [
        crate::fault::Point::IntendBefore,
        crate::fault::Point::IntendAfter,
        crate::fault::Point::BeginBefore,
        crate::fault::Point::BeginAdmitted,
        crate::fault::Point::AppliedBefore,
        crate::fault::Point::AppliedAfter,
    ] {
        let home = home();
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut fake = Fake::new(home.path());
        let mut parent = begin(&cx, &root, owner(), 224, &mut fake);
        let child_owner = ResourceId::store(Scope::Checkout(checkout()), "child").unwrap();
        crate::fault::arm_point(point, crate::fault::Fault::Enospc);
        assert!(parent
            .begin_child(&inv(child_owner), opts(225), &cleanup(&cx), &mut fake)
            .is_err());
        crate::fault::arm(None);
        drop(parent);
        let outcome = settle_interrupted(
            &cx,
            &root,
            &owner(),
            id(224),
            next(),
            &cleanup(&cx),
            &mut fake,
        )
        .unwrap();
        assert_eq!(outcome.cleanup(), CleanupOutcome::Complete, "{point:?}");
    }
}
