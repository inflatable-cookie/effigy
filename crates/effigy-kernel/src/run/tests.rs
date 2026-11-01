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
/// A fresh fixture home on the internal disk. These runs fsync at every journal
/// crossing and this suite's budget is calibrated to the internal disk's speed;
/// on a configured `[scratch]` volume the same work takes about twice as long
/// and the supervision matrix exceeds its per-test bound (effigy#326). The run's
/// own scratch directory is still exercised by the tests that name it.
pub(super) fn home() -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::in_parent("effigy-run-", Path::new("/tmp")).unwrap()
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

#[test]
fn an_environment_run_without_its_owner_gate_cannot_record_effects() {
    let fixture = effigy_test_support::Fixture::new("effigy-unguarded-env-").unwrap();
    let cx = context();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let mut fake = Fake::new(fixture.path());
    let mut run = begin(&cx, &root, env(), 209, &mut fake);
    let empty = Gate::acquire(&cx, &root, &[], Wait::FailFast).unwrap();
    let old = std::mem::replace(&mut run.guard, empty);
    drop(old);
    assert!(
        run.advance(Phase::Stopping).is_err(),
        "an unguarded environment run must not mutate"
    );
    assert_eq!(run.phase().unwrap(), Phase::Admitted);
    assert!(run
        .interrupt(&cleanup(&cx), WorkloadOutcome::NotStarted)
        .is_err());
    let (_, run) = run
        .settle_recoverable(&cleanup(&cx), &mut fake)
        .expect_err("settlement retains unguarded custody");
    assert_eq!(run.phase().unwrap(), Phase::Admitted);
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
/// A holder whose own effects precede its attachment (a detached run's
/// socket, a gateway, the `up` that attached it) settles them first and then
/// releases the attachment: settle visits attachments last, so the release
/// never meets an open effect of the same run.
#[test]
fn attachments_release_last_after_effects_intended_before_attaching() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let env_fence = active_environment(&cx, &root, &mut fake);
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    apply(&mut holder, &fake, "before-attach");
    Attachments::attach(&mut holder, &env_fence, AttachMode::Borrow, Wait::FailFast).unwrap();
    apply(&mut holder, &fake, "after-attach");
    holder.finish(WorkloadOutcome::Succeeded).unwrap();
    let outcome = holder.settle(&cleanup(&cx), &mut fake).unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    let view = reader(&cx, &root);
    let observed = cx.operation().observe().unwrap();
    assert!(matches!(
        Attachments::holders(&observed, &root, &view, &env()),
        Observed::Verified(ref holders, _) if holders.is_empty()
    ));
    let mut transition = begin(&cx, &root, env(), 91, &mut fake);
    assert!(Attachments::permit(&mut transition).is_ok());
    transition.settle(&cleanup(&cx), &mut fake).unwrap();
}
#[test]
fn attachments_residue_keeps_holder_protection_even_when_acknowledged() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let env_fence = active_environment(&cx, &root, &mut fake);
    let mut holder = begin(&cx, &root, owner(), 1, &mut fake);
    Attachments::attach(&mut holder, &env_fence, AttachMode::Borrow, Wait::FailFast).unwrap();
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

fn observe(cx: &EffectContext) -> crate::ctx::ObserveContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        cx.operation().limits().clone(),
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .unwrap()
}

fn log_read(
    cx: &EffectContext,
    root: &StateRoot,
    stream: crate::custody::Stream,
    offset: u64,
    max: usize,
) -> Observed<LogRead> {
    read_log(&observe(cx), root, id(40), 3, stream, offset, max)
}

#[test]
fn run_log_reads_resume_exactly_across_rotation_and_report_expiry() {
    use crate::custody::Stream;
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    assert!(matches!(
        log_read(&cx, &root, Stream::Stdout, 0, 64),
        Observed::Absent(_)
    ));
    // 4096-byte segments, two kept behind the open one: 12 KiB retained.
    let mut log = crate::custody::capture::Log::open(
        &root,
        id(40),
        3,
        Stream::Stdout,
        cx.operation().limits(),
    )
    .unwrap();
    let mut stderr = crate::custody::capture::Log::open(
        &root,
        id(40),
        3,
        Stream::Stderr,
        cx.operation().limits(),
    )
    .unwrap();
    let written: Vec<u8> = (0..10_000u32).map(|n| (n % 251) as u8).collect();
    for chunk in written.chunks(777) {
        log.append(chunk).unwrap();
    }
    stderr.append(b"err").unwrap();

    // A reader that stops anywhere resumes at exactly that byte.
    let mut resumed = Vec::new();
    let mut offset = 0;
    loop {
        let Observed::Verified(read, _) = log_read(&cx, &root, Stream::Stdout, offset, 1000) else {
            panic!("log is readable");
        };
        assert_eq!(read.from, offset, "nothing expired yet");
        assert_eq!(read.end, 10_000);
        if read.bytes.is_empty() {
            break;
        }
        offset += read.bytes.len() as u64;
        resumed.extend(read.bytes);
    }
    assert_eq!(resumed, written, "no gap and no duplicate across segments");
    let Observed::Verified(err, _) = log_read(&cx, &root, Stream::Stderr, 0, 64) else {
        panic!("stderr is its own log");
    };
    assert_eq!(err.bytes, b"err");

    // Rotation past the kept segments expires the oldest bytes: a reader at
    // offset 0 is told where the log now starts, never handed later bytes as
    // if they were first.
    log.append(&vec![b'x'; 8000]).unwrap();
    let Observed::Verified(read, _) = log_read(&cx, &root, Stream::Stdout, 0, 16) else {
        panic!("log is readable");
    };
    assert_eq!(read.end, 18_000);
    assert_eq!(read.from, 4096 * 2, "the first two segments expired");
    assert_eq!(read.bytes, written[8192..8192 + 16]);
    assert!(!read.truncated);
}

#[test]
fn run_log_write_failure_is_reported_as_truncation() {
    use crate::custody::Stream;
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut log = crate::custody::capture::Log::open(
        &root,
        id(40),
        3,
        Stream::Stdout,
        cx.operation().limits(),
    )
    .unwrap();
    log.append(b"kept").unwrap();
    log.mark_truncated();
    let Observed::Verified(read, _) = log_read(&cx, &root, Stream::Stdout, 0, 64) else {
        panic!("log is readable");
    };
    assert_eq!(read.bytes, b"kept");
    assert!(read.truncated, "a failed write is never a silent end");
}

#[test]
fn run_control_socket_long_checkout_path_has_bounded_probe_and_exact_cleanup() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture
        .path()
        .join(format!("long-checkout-{}", "x".repeat(180)));
    std::fs::create_dir(&path).unwrap();
    let cx = context();
    let root = StateRoot::effects(&cx, &path).unwrap();
    let mut fake = Fake::new(&path);
    let mut run = begin(&cx, &root, owner(), 63, &mut fake);
    let listener = run.control_socket().unwrap();
    let address = control_socket_path(&root, &owner(), run.id());
    assert!(address.as_os_str().len() > 200);
    let observe = cx
        .operation()
        .child(Duration::from_secs(1))
        .observe()
        .unwrap();
    assert_eq!(
        connect_control_socket(&observe, &root, &owner(), run.id()).verified(),
        Some(&crate::probe::ProbeIo::UnixOpen)
    );
    drop(listener.accept().unwrap());
    drop(listener);
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert!(!address.exists());
}

#[test]
fn gateway_control_socket_lives_in_trusted_runtime_and_settles_exact_inode() {
    let home = effigy_test_support::short_tempdir("g219-").unwrap();
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
    let observe = cx
        .operation()
        .child(Duration::from_secs(1))
        .observe()
        .unwrap();
    assert_eq!(
        connect_control_socket(&observe, &root, &resource, run.id()).verified(),
        Some(&crate::probe::ProbeIo::UnixOpen)
    );
    drop(listener.accept().unwrap());
    drop(listener);
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert!(!path.exists());
}
#[test]
fn gateway_operator_events_write_one_private_record_per_action() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = effigy_test_support::Fixture::new("gateway-provenance-").unwrap();
    let directory = fixture.path().join("gateway");
    // No gateway state yet: recording is read-only and creates nothing.
    record_gateway_operator(&directory, OperatorAction::Up, "gateway up command");
    assert!(!directory.exists());
    std::fs::create_dir(&directory).unwrap();
    for (action, cause) in [
        (OperatorAction::Up, "gateway up command"),
        (OperatorAction::Down, "gateway down command"),
        (OperatorAction::Setup, "gateway setup command"),
    ] {
        record_gateway_operator(&directory, action, cause);
    }
    let path = directory.join("gateway.stderr.log");
    let log = std::fs::read_to_string(&path).unwrap();
    let events = log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 3, "{log}");
    for (event, (name, cause)) in events.iter().zip([
        ("gateway_up", "gateway up command"),
        ("gateway_stop", "gateway down command"),
        ("gateway_setup", "gateway setup command"),
    ]) {
        assert_eq!(event["event"], name, "{log}");
        assert_eq!(event["cause"], cause, "{log}");
        assert_eq!(event["pid"], std::process::id(), "{log}");
        assert_eq!(event["caller"]["pid"], std::process::id(), "{log}");
        assert!(event["caller"]["markers"].is_object(), "{log}");
    }
    // A stop keeps the event name earlier records already use.
    assert_eq!(events[1]["action"], "down", "{log}");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "an operator log stays private");
}

#[test]
fn gateway_custody_settlement_logs_caller_and_owner_loss() {
    let home = effigy_test_support::Fixture::new("g382-").unwrap();
    let cx = context();
    let root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let directory = home.path().join(".effigy/gateway");
    std::fs::create_dir_all(&directory).unwrap();
    let mut fake = Fake::new(home.path());
    let resource = ResourceId::gateway("operator").unwrap();
    let mut specification = inv(resource).spec().clone();
    specification.task = TaskId::new(
        CheckoutId::new(CheckoutKind::Machine, [0; 16]),
        "",
        Surface::Published,
        "gateway",
        None,
    )
    .unwrap();
    let invocation = ResolvedInvocation::new(specification).unwrap();
    let mut run = Run::begin(&cx, &root, &invocation, opts(82), &cleanup(&cx), &mut fake).unwrap();
    run.interrupt(
        &cleanup(&cx),
        WorkloadOutcome::Cancelled {
            cause: crate::ctx::StopCause::OwnerLost,
            signal: None,
        },
    )
    .unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    let log = std::fs::read_to_string(directory.join("gateway.stderr.log")).unwrap();
    let record: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
    assert_eq!(record["event"], "gateway_stop");
    assert_eq!(record["pid"], std::process::id());
    assert_eq!(
        record["cwd"],
        std::env::current_dir().unwrap().to_string_lossy().as_ref()
    );
    assert!(record["argv"]
        .as_array()
        .is_some_and(|args| !args.is_empty()));
    assert!(record["cause"]
        .as_str()
        .unwrap()
        .contains("the owning process exiting"));
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
/// A settle can stop on its last open effect, where no later effect carries the
/// reason. The settled outcome must still name that effect, its own sequence,
/// the redacted label its intent recorded and the custody reason — and it must
/// tell two otherwise identical setup effects apart, because every gateway
/// setup adapter command shares one generic label (effigy#476).
#[test]
fn gateway_settle_names_the_final_unresolved_custody_effect_and_reason() {
    use crate::custody;
    use crate::outcome::ErrorCode;
    let reason = crate::observe::UnknownReason::Inconsistent("group anchor unavailable".into());
    let effect = |sequence: i64| Effect {
        sequence,
        spec: EffectSpec {
            target: Fence {
                resource: ResourceId::store(Scope::Machine, "gateway-setup").unwrap(),
                generation: Generation::new(1),
            },
            kind: custody::KIND.into(),
            payload: serde_json::json!({"digest": "fixture", "label": "gateway setup adapter"}),
            settlement: Settlement::Release(Value::Null),
            required: true,
        },
        state: EffectState::Residue,
        receipt: None,
        reason: Some(reason.to_string()),
    };
    let settled = settle::outcome(
        id(5),
        &next(),
        WorkloadOutcome::Succeeded,
        vec![effect(5), effect(6)],
    )
    .unwrap();
    assert_eq!(settled.cleanup(), CleanupOutcome::Incomplete);
    assert!(matches!(
        settled.result(),
        Err(error) if error.code == ErrorCode::Incomplete
    ));
    let unresolved = settled.unresolved();
    assert_eq!(unresolved.len(), 2, "{unresolved:?}");
    let [fifth, sixth] = [&unresolved[0], &unresolved[1]];
    for text in [fifth, sixth] {
        assert!(text.contains("`kernel:process`"), "{text}");
        assert!(text.contains("gateway setup adapter"), "{text}");
        assert!(text.contains("group anchor unavailable"), "{text}");
        // The rendered reason appears once: a residue detail is not re-wrapped.
        assert_eq!(
            text.matches("group anchor unavailable").count(),
            1,
            "{text}"
        );
    }
    assert!(fifth.contains("effect 5"), "{fifth}");
    assert!(sixth.contains("effect 6"), "{sixth}");
    assert_ne!(
        fifth, sixth,
        "otherwise identical setup effects stay distinguishable"
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
    drop(reader);
    let run = begin(&cx, &root, resource.clone(), 64, &mut fake);
    // Each observation reads the store as it is: the next read sees the new
    // generation without keeping a filesystem connection open.
    let fresh = Store::read(&observe, &root);
    assert_eq!(
        fresh
            .verified()
            .unwrap()
            .latest_run(&resource)
            .verified()
            .unwrap()
            .0,
        id(64)
    );
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

#[test]
fn run_checkout_aliases_remain_exclusive_until_the_logical_owner_closes() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let alias = TaskId::new(
        CheckoutId::new(CheckoutKind::Primary, [2; 16]),
        "",
        Surface::Published,
        "task",
        None,
    )
    .unwrap()
    .resource();
    let mut spec = inv(owner()).spec().clone();
    spec.gates.push(alias.clone());
    let invocation = ResolvedInvocation::new(spec).unwrap();
    let mut fake = Fake::new(home.path());
    let mut run = Run::begin(&cx, &root, &invocation, opts(1), &cleanup(&cx), &mut fake).unwrap();
    assert!(matches!(
        Gate::probe(&cx.operation().observe().unwrap(), &root, &alias),
        Observed::Verified(crate::gate::GateState::Held(_), _)
    ));
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
    assert!(matches!(
        Gate::probe(&cx.operation().observe().unwrap(), &root, &alias),
        Observed::Verified(crate::gate::GateState::Free, _)
    ));
}

#[test]
fn run_machine_owner_refusal_precedes_database_creation_and_shared_runs_reject_payloads() {
    let home = effigy_test_support::Fixture::new("kernel-machine-model-").unwrap();
    let cx = context();
    let root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut spec = inv(owner()).spec().clone();
    for (resource, machine_task) in [
        (owner(), false),
        (env(), true),
        (
            ResourceId::store(Scope::Machine, "release arbitrary").unwrap(),
            true,
        ),
        (ResourceId::gateway("fixture").unwrap(), false),
    ] {
        spec.owner = resource;
        if machine_task {
            spec.task = TaskId::new(
                CheckoutId::new(CheckoutKind::Machine, [0; 16]),
                "",
                Surface::Published,
                "gateway",
                None,
            )
            .unwrap();
        } else {
            spec.task = inv(owner()).spec().task.clone();
        }
        let invocation = ResolvedInvocation::new(spec.clone()).unwrap();
        let result = Run::begin(
            &cx,
            &root,
            &invocation,
            BeginOpts {
                id: id(1),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: None,
            },
            &cleanup(&cx),
            &mut fake,
        );
        assert!(matches!(result, Err(RunError::Refused(_))));
        assert!(!root.path().join("machine.db").exists());
    }
    spec.owner = ResourceId::gateway("fixture").unwrap();
    spec.task = TaskId::new(
        CheckoutId::new(CheckoutKind::Machine, [0; 16]),
        "",
        Surface::Published,
        "gateway",
        None,
    )
    .unwrap();
    let invocation = ResolvedInvocation::new(spec).unwrap();
    let mut run = Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: id(2),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: None,
        },
        &cleanup(&cx),
        &mut fake,
    )
    .unwrap();
    let before = run.effects().unwrap().len();
    let closure = crate::objects::Closure::new(BTreeMap::from([(
        "payload".into(),
        b"checkout only".to_vec(),
    )]))
    .unwrap();
    assert!(crate::objects::Objects::publish_desired(&mut run, &closure).is_err());
    assert!(crate::objects::Objects::promote(&mut run, closure.digest()).is_err());
    assert!(crate::objects::Objects::reset(&mut run).is_err());
    assert_eq!(run.effects().unwrap().len(), before);
    assert!(run
        .capture_log(1, crate::custody::Stream::Stdout, cx.operation().limits())
        .unwrap()
        .is_none());
    for family in ["objects", "staging", "runs"] {
        assert!(!root.path().join(family).exists());
    }
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
}

/// The narrow Q-010 exception's row (effigy#325): one terminal result per v2
/// hook-admitted foreground run, read back by run id, newest first, and
/// absent — never unreadable — from a journal written before the table
/// existed. The row carries no process identity to reclaim from.
#[test]
fn run_terminal_record_round_trips_and_an_older_journal_reads_absent() {
    let fixture = home();
    let cx = context();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let observe = cx.child(Duration::from_secs(10));
    let observe = observe.operation().observe().unwrap();

    // A journal written before this exception has no such table, and that is
    // absence, not an unreadable journal.
    {
        let store = Store::open(&cx, &root).unwrap();
        store
            .conn()
            .execute_batch("DROP TABLE foreground_terminals")
            .unwrap();
    }
    let reader = match Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal: {other:?}"),
    };
    assert!(matches!(
        terminal_record(&reader, id(9)),
        Observed::Absent(AbsenceProof::NoRow { .. })
    ));

    // The next writer adds the additive table and records one row.
    let record = TerminalRecord {
        run_id: id(9).to_string(),
        checkout: checkout().text(),
        boot: "20a888e4-10fc-4798-a139-fe8e08758c34".into(),
        task: "dev".into(),
        outcome: "succeeded".into(),
        exit_code: Some(0),
        signal: None,
        cleanup: "complete".into(),
        workload_groups: 3,
        // A closed time inside the retention window: an older record is
        // pruned by its own write, which the CLI test covers.
        closed_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default(),
    };
    record_terminal(&cx, &root, &record).unwrap();
    // Repeating a settle keeps the first evidence.
    let mut relayed = record.clone();
    relayed.outcome = "failed".into();
    record_terminal(&cx, &root, &relayed).unwrap();

    let reader = match Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("journal: {other:?}"),
    };
    let read = match terminal_record(&reader, id(9)) {
        Observed::Verified(Some(read), _) => read,
        other => panic!("record: {other:?}"),
    };
    assert_eq!(read, record);
    // Evidence for the provider's lease only: nothing here names a process.
    let encoded = serde_json::to_value(&read).unwrap();
    let keys: Vec<&str> = encoded
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "boot",
            "checkout",
            "cleanup",
            "closed_at_ms",
            "exit_code",
            "outcome",
            "run_id",
            "signal",
            "task",
            "workload_groups"
        ],
        "{encoded}"
    );
    assert!(matches!(
        terminal_record(&reader, id(8)),
        Observed::Absent(AbsenceProof::NoRow { .. })
    ));
    match terminal_records(&reader, 10) {
        Observed::Verified(records, _) => assert_eq!(records, vec![record]),
        other => panic!("records: {other:?}"),
    }
}

/// A settlement refusal names an effect's state in words. The variant name a
/// `Debug` rendering shows (`Unknown`, `Residue`) never reaches a person
/// (effigy#402, Q-017).
#[test]
fn effect_states_read_as_words_in_a_refusal() {
    assert_eq!(EffectState::Unknown.label(), "unknown");
    assert_eq!(EffectState::Residue.label(), "left as residue");
    assert_eq!(EffectState::Failed.label(), "failed");
    let labels = [
        EffectState::Intended,
        EffectState::Applied,
        EffectState::Failed,
        EffectState::Unknown,
        EffectState::Released,
        EffectState::Retained,
        EffectState::Irreversible,
        EffectState::Residue,
    ]
    .map(EffectState::label);
    for label in labels {
        assert!(!label.is_empty(), "every state names itself");
        assert!(
            label.chars().next().is_some_and(|c| c.is_ascii_lowercase()),
            "a label reads as prose, not as a variant: {label}"
        );
    }
}

#[test]
fn guest_publication_is_typed_owner_bound_and_required_before_active() {
    let home = effigy_test_support::Fixture::new("guest-publication-kernel-").unwrap();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut fake = Fake::new(home.path());
    let mut task = begin(&cx, &root, owner(), 60, &mut fake);
    assert!(Attachments::intend_guest_publication(&mut task).is_err());
    task.finish(WorkloadOutcome::Succeeded).unwrap();
    task.settle(&cleanup(&cx), &mut fake).unwrap();
    let mut run = begin(&cx, &root, env(), 61, &mut fake);
    let ticket = Attachments::intend_guest_publication(&mut run).unwrap();
    assert!(Attachments::publish_active(&mut run).is_err());
    let mut receipt = GuestPublicationReceipt {
        environment: run.fence().clone(),
        container_id: "a".repeat(64),
        vm_boot_id: "boot-a".into(),
        artifact_digest: format!("sha256:{}", "b".repeat(64)),
        context_generation: 0,
    };
    assert!(Attachments::guest_published(&mut run, &ticket, receipt.clone()).is_err());
    assert!(Attachments::publish_active(&mut run).is_err());
    // Raw environment intents and reserved kinds remain forbidden.
    assert!(run
        .intend(EffectSpec {
            target: run.fence().clone(),
            kind: GUEST_PUBLICATION_KIND.into(),
            payload: Value::Null,
            settlement: Settlement::Retain,
            required: false
        })
        .is_err());
    receipt.context_generation = 1;
    Attachments::guest_published(&mut run, &ticket, receipt).unwrap();
    Attachments::publish_active(&mut run).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cleanup(&cx), &mut fake).unwrap();
}
