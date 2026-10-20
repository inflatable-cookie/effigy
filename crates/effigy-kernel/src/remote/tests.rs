use super::*;
use crate::ctx::*;
use crate::id::*;
use crate::observe::AbsenceProof;
use crate::outcome::{CleanupOutcome, WorkloadOutcome};
use crate::root::StateRoot;
use crate::run::*;
use crate::strict_fake::*;
use crate::supervise::{Executable, LifecyclePlan};
use serde_json::json;
use std::path::{Path, PathBuf};

fn cx() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 1024,
            queue_bytes: 8192,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap()
}
fn container() -> ContainerIdentity {
    ContainerIdentity {
        id: "full-container-a".into(),
        created: "creation-a".into(),
        vm_boot_id: "boot-a".into(),
    }
}
fn env() -> Fence {
    Fence {
        resource: ResourceId::environment(
            CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]),
            "remote",
        )
        .unwrap(),
        generation: Generation::new(1),
    }
}
fn reg() -> RemoteRegistration {
    RemoteRegistration {
        env: env(),
        container: container(),
        pid_ns: 401,
        sid: 42,
        leader_pid: 42,
        leader_start_ticks: 100,
        nonce: Nonce("a".repeat(64)),
        session_isolated: true,
    }
}
fn intent(reg: &RemoteRegistration) -> Intent {
    Intent {
        env: reg.env.clone(),
        container: reg.container.clone(),
        nonce: reg.nonce.clone(),
        grace_millis: 0,
        call_millis: 1000,
    }
}
fn effect(reg: &RemoteRegistration, receipt: bool) -> Effect {
    Effect {
        sequence: 1,
        spec: EffectSpec {
            target: reg.env.clone(),
            kind: KIND.into(),
            payload: serde_json::to_value(intent(reg)).unwrap(),
            settlement: Settlement::Release(json!(null)),
            required: true,
        },
        state: if receipt {
            EffectState::Applied
        } else {
            EffectState::Intended
        },
        receipt: receipt.then(|| serde_json::to_value(reg).unwrap()),
        reason: None,
    }
}
fn step<C, R>(call: C, reply: R) -> Step<C, R> {
    Step {
        call,
        reply,
        effects: vec![],
    }
}
fn remote(steps: Vec<Step<RemoteCall, RemoteReply>>) -> StrictRemote {
    StrictRemote {
        tape: StrictFake::new(steps, BTreeMap::from([("foreign".into(), json!("alive"))])),
        launches: vec![],
    }
}
fn session(state: SessionState) -> RemoteReply {
    RemoteReply::Session(Observed::Verified(state, evidence()))
}
fn closed_steps(r: &RemoteRegistration) -> Vec<Step<RemoteCall, RemoteReply>> {
    vec![
        step(
            RemoteCall::Observe(r.clone()),
            session(SessionState::Running),
        ),
        step(
            RemoteCall::Signal(r.clone(), Signal::Term),
            session(SessionState::Running),
        ),
        step(
            RemoteCall::Signal(r.clone(), Signal::Continue),
            session(SessionState::Running),
        ),
        step(
            RemoteCall::Observe(r.clone()),
            session(SessionState::Running),
        ),
        step(
            RemoteCall::Signal(r.clone(), Signal::Kill),
            session(SessionState::Running),
        ),
        step(
            RemoteCall::Observe(r.clone()),
            session(SessionState::Closed),
        ),
    ]
}
#[test]
fn in_band_frames_are_stripped_before_all_sinks_and_bound_to_identity() {
    let r = reg();
    let i = intent(&r);
    let frame = format!(
        "\x1eEFFIGY-CUSTODY v1 {} 42 42 100 boot-a 401\n",
        r.nonce.as_str()
    );
    for split in 0..=frame.len() {
        let mut s = stream::RegistrationStream::new(i.clone(), false);
        assert_eq!(s.push(b"ordinary\n", false), b"ordinary\n");
        let mut output = s.push(&frame.as_bytes()[..split], false);
        output.extend(s.push(&frame.as_bytes()[split..], false));
        output.extend(s.push(b"workload", true));
        assert_eq!(output, b"workload");
        assert_eq!(s.registration(), Some(Ok(r.clone())));
    }
    for frame in [
        frame.replace("boot-a", "boot-b"),
        frame.replace("42 42", "0 42"),
        frame.replace("v1", "v2"),
        frame.replace(r.nonce.as_str(), "wrong"),
        frame.replace("100 boot", "0 boot"),
        format!("\x1eEFFIGY-CUSTODY {}\n", "a".repeat(3000)),
    ] {
        let mut s = stream::RegistrationStream::new(i.clone(), false);
        assert!(s.push(frame.as_bytes(), true).is_empty());
        assert!(s.registration().unwrap().is_err());
    }
    let mut s = stream::RegistrationStream::new(i, false);
    assert!(s.push(b"\x1eEFFIGY-CUSTODY v1 unfinished", true).is_empty());
    assert!(s.registration().unwrap().is_err());
}
#[test]
fn settlement_observes_remote_after_kill_and_rejects_unknown_or_foreign_proofs() {
    let cx = cx();
    let clean = cx.operation().cleanup(Duration::from_secs(3)).unwrap();
    let r = reg();
    let mut fake = remote(closed_steps(&r));
    assert_eq!(
        settle_effect(&effect(&r, true), &clean, Some(&mut fake)).0,
        EffectState::Released
    );
    fake.tape.assert_finished();
    for reason in [
        UnknownReason::Injected,
        UnknownReason::TimedOut {
            phase: "backend".into(),
        },
        UnknownReason::PermissionDenied,
    ] {
        let mut fake = remote(vec![step(
            RemoteCall::Observe(r.clone()),
            RemoteReply::Session(Observed::Unknown(reason)),
        )]);
        assert_eq!(
            settle_effect(&effect(&r, true), &clean, Some(&mut fake)).0,
            EffectState::Residue
        );
        fake.tape.assert_finished();
    }
    for proof in [
        AbsenceProof::ContainerGone {
            id: "replacement".into(),
        },
        AbsenceProof::NoSuchProcess { pid: 42 },
        AbsenceProof::BootChanged {
            recorded: "other".into(),
            current: "new".into(),
        },
    ] {
        let mut fake = remote(vec![step(
            RemoteCall::Observe(r.clone()),
            RemoteReply::Session(Observed::Absent(proof)),
        )]);
        assert_eq!(
            settle_effect(&effect(&r, true), &clean, Some(&mut fake)).0,
            EffectState::Residue
        );
    }
    // A positive, complete inventory without the nonce proves the workload
    // never started (R5); an unobservable one stays residue under K5.
    let mut fake = remote(vec![step(
        RemoteCall::Find(container(), r.nonce.clone()),
        RemoteReply::Find(Observed::Verified(None, evidence())),
    )]);
    assert_eq!(
        settle_effect(&effect(&r, false), &clean, Some(&mut fake)).0,
        EffectState::Released
    );
    fake.tape.assert_finished();
    let mut fake = remote(vec![step(
        RemoteCall::Find(container(), r.nonce.clone()),
        RemoteReply::Find(Observed::Unknown(UnknownReason::PermissionDenied)),
    )]);
    assert_eq!(
        settle_effect(&effect(&r, false), &clean, Some(&mut fake)).0,
        EffectState::Residue
    );
    fake.tape.assert_finished();
}
#[test]
fn remote_signals_require_live_leader_or_member_nonce_and_start_ticks() {
    use session::*;
    let r = reg();
    let member = Member {
        pid: 43,
        start_ticks: 101,
        pgid: 42,
        sid: 42,
        nonce: Observed::Verified(r.nonce.0.clone(), evidence()),
        descendant: true,
    };
    let mut s = Snapshot {
        container: container(),
        pid_ns: 401,
        leader: None,
        members: vec![member.clone()],
    };
    assert!(
        matches!(plan_signal(&r,&s),Observed::Verified(v,_) if matches!(&v[..],[SignalTarget::Member{pid:43,..}]))
    );
    for bad in [
        Member {
            nonce: Observed::Unknown(UnknownReason::PermissionDenied),
            ..member.clone()
        },
        Member {
            start_ticks: 99,
            ..member.clone()
        },
        Member {
            nonce: Observed::Verified("foreign".into(), evidence()),
            ..member.clone()
        },
    ] {
        s.members = vec![bad];
        assert!(matches!(plan_signal(&r, &s), Observed::Unknown(_)));
    }
    s.members = vec![member];
    s.leader = Some(Member {
        pid: 42,
        start_ticks: 100,
        pgid: 42,
        sid: 42,
        nonce: Observed::Unknown(UnknownReason::PermissionDenied),
        descendant: false,
    });
    assert!(
        matches!(plan_signal(&r,&s),Observed::Verified(v,_) if matches!(&v[..],[SignalTarget::Session{..}]))
    );
    s.leader.as_mut().unwrap().start_ticks = 200;
    assert!(
        matches!(plan_signal(&r,&s),Observed::Verified(v,_) if matches!(&v[..],[SignalTarget::Member{..}]))
    );
    s.members.clear();
    assert!(matches!(
        observe(&r, Observed::Verified(s.clone(), evidence())),
        Observed::Verified(SessionState::Closed, _)
    ));
    s.container.vm_boot_id = "boot-b".into();
    assert!(matches!(
        observe(&r, Observed::Verified(s.clone(), evidence())),
        Observed::Absent(AbsenceProof::BootChanged { .. })
    ));
    s.container = ContainerIdentity {
        id: "replacement".into(),
        ..container()
    };
    assert!(matches!(plan_signal(&r, &s), Observed::Unknown(_)));
    s.container = container();
    s.pid_ns = 999;
    assert!(matches!(plan_signal(&r, &s), Observed::Unknown(_)));
}
struct Adapter(StrictRemote);
impl SettlementAdapter for Adapter {
    fn remote(&mut self) -> Option<&mut dyn RemoteExecAdapter> {
        Some(&mut self.0)
    }
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        panic!("unmodelled observe")
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        panic!("unmodelled release")
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        panic!("unmodelled reconcile")
    }
}
fn begin<'a>(cx: &EffectContext, root: &'a StateRoot, adapter: &mut Adapter) -> Run<'a> {
    let mut store = crate::store::Store::open(cx, root).unwrap();
    store
        .txn(cx, |txn| {
            txn.resource(&env().resource, "environment")?;
            if txn.conn().query_row(
                "SELECT generation FROM resources WHERE name=?1",
                [env().resource.name().to_string()],
                |r| r.get::<_, i64>(0),
            )? == 0
            {
                txn.next_generation(&env().resource)?;
            }
            Ok(())
        })
        .unwrap();
    let mut opts = BeginOpts {
        id: RunId::new(1, [1; 10]).unwrap(),
        wait: crate::gate::Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: None,
    };
    opts.custody = Some(LifecyclePlan {
        executable: Executable::capture(&fixture()).unwrap(),
        placement: crate::supervise::Placement::Inline,
    });
    Run::begin(
        cx,
        root,
        &inv(),
        opts,
        &cx.operation().cleanup(Duration::from_secs(4)).unwrap(),
        adapter,
    )
    .unwrap()
}
fn fixture() -> PathBuf {
    std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("kernel-custody-fixture")
        })
}
fn expected(run: &Run<'_>) -> RemoteRegistration {
    let bytes = serde_json::to_vec(&(run.id(), run.fence(), run.effects().unwrap().len())).unwrap();
    RemoteRegistration {
        nonce: Nonce(
            Digest::of(&bytes)
                .0
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        ),
        ..reg()
    }
}
fn model_launch(home: &Path, ack: bool, keep_client: bool) -> RemoteReply {
    let cwd = home.to_owned();
    RemoteReply::Launch(Box::new(move |command| {
        assert_eq!(command.argv, vec![OsString::from("workload")]);
        assert!(matches!(
            command.env.get(&OsString::from("EFFIGY_CUSTODY_NONCE")),
            Some(EnvValue::Secret(_))
        ));
        // The exec client and guest have distinct custody nonces.
        let mut env = command.env;
        let remote_nonce = env.remove(&OsString::from("EFFIGY_CUSTODY_NONCE")).unwrap();
        env.insert("TEST_REMOTE_NONCE".into(), remote_nonce);
        let script = if ack && keep_client {
            "printf '\\036EFFIGY-CUSTODY v1 %s 42 42 100 boot-a 401\\n' \"$TEST_REMOTE_NONCE\" >&2; exec /bin/sleep 60"
        } else if ack {
            "printf '\\036EFFIGY-CUSTODY v1 %s 42 42 100 boot-a 401\\n' \"$TEST_REMOTE_NONCE\" >&2; printf 'user-output\\n' >&2; exit 0"
        } else {
            "exec /bin/sleep 60"
        };
        Ok(ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cwd,
            env,
            grace: Duration::from_millis(20),
            lifetime: crate::custody::Lifetime::Finite(Duration::from_secs(5)),
            terminal: Terminal::None,
            label: "remote fake exec".into(),
        })
    }))
}
fn launch_spec() -> RemoteLaunch {
    RemoteLaunch {
        env: env(),
        target: ContainerTarget("selected".into()),
        command: RemoteCommand {
            argv: vec!["workload".into()],
            env: BTreeMap::new(),
            terminal: RemoteTerminal::None,
        },
        ack_timeout: Duration::from_secs(1),
        call_timeout: Duration::from_secs(1),
        grace: Duration::ZERO,
    }
}
fn launch_steps(home: &Path, ack: bool) -> Vec<Step<RemoteCall, RemoteReply>> {
    vec![
        step(
            RemoteCall::Identity(ContainerTarget("selected".into())),
            RemoteReply::Identity(Observed::Verified(container(), evidence())),
        ),
        step(
            RemoteCall::Launch(container()),
            model_launch(home, ack, false),
        ),
    ]
}
#[test]
fn journal_first_launch_strips_ack_and_local_exit_is_not_remote_completion() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut a = Adapter(remote(launch_steps(home.path(), true)));
    let mut run = begin(&cx, &root, &mut a);
    let expected = expected(&run);
    let mut remote = RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut a.0).unwrap();
    assert_eq!(remote.registration, expected);
    let exit = remote
        .client()
        .wait(&cx.operation().child(Duration::from_secs(1)));
    let exit = *exit.verified().unwrap();
    assert_eq!(exit.code, 0);
    remote.client().complete(&mut run, exit).unwrap();
    assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
    assert_eq!(remote.client().take_output(), b"user-output\n");
    assert!(!remote
        .client()
        .tail()
        .windows(14)
        .any(|s| s == b"EFFIGY-CUSTODY"));
    a.0.tape.expect(step(
        RemoteCall::Observe(expected),
        RemoteReply::Session(Observed::Unknown(UnknownReason::Injected)),
    ));
    let outcome = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
            &mut a,
        )
        .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Incomplete);
    a.0.tape.assert_finished();
}
#[test]
fn unknown_identity_and_guest_placement_never_launch_or_journal() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut a = Adapter(remote(vec![step(
        RemoteCall::Identity(ContainerTarget("selected".into())),
        RemoteReply::Identity(Observed::Unknown(UnknownReason::Injected)),
    )]));
    let mut run = begin(&cx, &root, &mut a);
    let before = run.effects().unwrap().len();
    assert!(RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut a.0).is_err());
    assert_eq!(run.effects().unwrap().len(), before);
    a.0.tape.assert_finished();
    let guest = OperationContext::new(
        cx.operation().deadline(),
        CancelToken::default(),
        cx.operation().limits().clone(),
        EffectClass::Mutate,
        Placement::Guest,
    )
    .effects()
    .unwrap();
    assert!(RemoteCustody::launch(&mut run, launch_spec(), &guest, &mut a.0).is_err());
    assert_eq!(a.0.tape.calls().len(), 1);
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
        &mut a,
    )
    .unwrap();
}
#[test]
fn enospc_at_each_remote_boundary_keeps_nonce_for_the_one_settle() {
    for point in [
        crate::fault::Point::RemoteBeforeAck,
        crate::fault::Point::RemoteAfterAckBeforeRecord,
    ] {
        let home = tempfile::tempdir().unwrap();
        let cx = cx();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut a = Adapter(remote(launch_steps(home.path(), true)));
        let mut run = begin(&cx, &root, &mut a);
        let r = expected(&run);
        crate::fault::arm_point(point, crate::fault::Fault::Enospc);
        let result = RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut a.0);
        let crossed = crate::fault::trace();
        crate::fault::arm(None);
        assert!(
            crossed.contains(&point),
            "fault point was not reached: {point:?}: {result:?}"
        );
        assert!(result.is_err());
        let effects = run.effects().unwrap();
        let entry = effects.iter().find(|e| e.spec.kind == KIND).unwrap();
        assert_eq!(entry.state, EffectState::Intended);
        assert!(entry.receipt.is_none());
        a.0.tape.expect(step(
            RemoteCall::Find(container(), r.nonce.clone()),
            RemoteReply::Find(Observed::Verified(Some(r.clone()), evidence())),
        ));
        a.0.tape
            .expect(step(RemoteCall::Observe(r), session(SessionState::Closed)));
        let outcome = run
            .settle(
                &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
                &mut a,
            )
            .unwrap();
        assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
        a.0.tape.assert_finished();
    }
}
#[test]
fn ack_deadline_and_partial_launch_effect_require_nonce_recovery() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut a = Adapter(remote(launch_steps(home.path(), false)));
    let mut run = begin(&cx, &root, &mut a);
    let r = expected(&run);
    let mut spec = launch_spec();
    spec.ack_timeout = Duration::from_millis(15);
    assert!(matches!(
        RemoteCustody::launch(&mut run, spec, &cx, &mut a.0),
        Err(RemoteError::Unknown(UnknownReason::TimedOut { .. }))
    ));
    a.0.tape.expect(step(
        RemoteCall::Find(container(), r.nonce.clone()),
        RemoteReply::Find(Observed::Unknown(UnknownReason::PermissionDenied)),
    ));
    let outcome = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
            &mut a,
        )
        .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Incomplete);
    a.0.tape.assert_finished();
}
#[test]
fn conformance_runner_covers_every_adapter_case_with_strict_fakes() {
    use conformance::*;
    struct Model {
        fake: StrictRemote,
        case: Option<Case>,
    }
    impl Fixture for Model {
        fn arrange(&mut self, case: Case, _: &OperationContext) -> Result<Option<Effect>, String> {
            self.fake.tape.assert_finished();
            self.case = Some(case);
            if case == Case::BeforeIdentity {
                return Ok(None);
            }
            let mut r = reg();
            if case == Case::NoSetsid {
                r.sid = 7;
                r.session_isolated = false;
            }
            let receipt = !matches!(
                case,
                Case::Intended | Case::ClientSpawned | Case::Acknowledged
            );
            let mut steps = vec![];
            if !receipt {
                steps.push(step(
                    RemoteCall::Find(container(), r.nonce.clone()),
                    RemoteReply::Find(Observed::Verified(Some(r.clone()), evidence())),
                ));
            }
            match case {
                Case::ContainerReplaced => steps.push(step(
                    RemoteCall::Observe(r.clone()),
                    RemoteReply::Session(Observed::Absent(AbsenceProof::ContainerGone {
                        id: container().id,
                    })),
                )),
                Case::VmRestarted => steps.push(step(
                    RemoteCall::Observe(r.clone()),
                    RemoteReply::Session(Observed::Absent(AbsenceProof::BootChanged {
                        recorded: container().vm_boot_id,
                        current: "boot-b".into(),
                    })),
                )),
                Case::ForeignSession => steps.push(step(
                    RemoteCall::Observe(r.clone()),
                    RemoteReply::Session(Observed::Unknown(inconsistent(
                        "foreign session has no nonce",
                    ))),
                )),
                Case::BackendHung => steps.push(step(
                    RemoteCall::Observe(r.clone()),
                    RemoteReply::Session(Observed::Unknown(UnknownReason::TimedOut {
                        phase: "backend".into(),
                    })),
                )),
                Case::GuestPermissionDenied => steps.push(step(
                    RemoteCall::Observe(r.clone()),
                    RemoteReply::Session(Observed::Unknown(UnknownReason::PermissionDenied)),
                )),
                _ => steps.extend(closed_steps(&r)),
            }
            self.fake = remote(steps);
            Ok(Some(effect(&r, receipt)))
        }
        fn adapter(&mut self) -> &mut dyn RemoteExecAdapter {
            &mut self.fake
        }
        fn verify_oracles(
            &mut self,
            case: Case,
            _: bool,
            _: &OperationContext,
        ) -> Result<(), String> {
            assert_eq!(self.case, Some(case));
            self.fake.tape.assert_finished();
            assert_eq!(self.fake.tape.resources()["foreign"], json!("alive"));
            let unselected = StrictBackend::new(vec![], BTreeMap::new());
            unselected.assert_unselected();
            Ok(())
        }
    }
    let mut model = Model {
        fake: remote(vec![]),
        case: None,
    };
    let cx = cx();
    let report = run(
        &mut model,
        &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
    )
    .unwrap();
    assert_eq!(report.len(), CASES.len());
}
#[test]
fn fallback_without_setsid_and_foreign_delivery_races_never_signal_unknown() {
    use session::*;
    let mut r = reg();
    r.sid = 7;
    r.session_isolated = false;
    let ours = Member {
        pid: 42,
        start_ticks: 100,
        pgid: 8,
        sid: 7,
        nonce: Observed::Verified(r.nonce.0.clone(), evidence()),
        descendant: false,
    };
    let mut snapshot = Snapshot {
        container: container(),
        pid_ns: 401,
        leader: Some(ours.clone()),
        members: vec![
            ours.clone(),
            Member {
                pid: 43,
                descendant: true,
                pgid: 9,
                ..ours.clone()
            },
        ],
    };
    assert!(
        matches!(plan_signal(&r,&snapshot),Observed::Verified(v,_) if v.len()==2 && v.iter().all(|t|matches!(t,SignalTarget::Member{..})))
    );
    snapshot.members.push(Member {
        pid: 99,
        start_ticks: 200,
        nonce: Observed::Verified("foreign".into(), evidence()),
        ..ours
    });
    #[derive(Debug)]
    struct FakeTransport {
        snapshot: Snapshot,
        deliveries: usize,
        race: bool,
    }
    impl Transport for FakeTransport {
        fn snapshot(&mut self, _: &RemoteRegistration, _: &OperationContext) -> Observed<Snapshot> {
            Observed::Verified(self.snapshot.clone(), evidence())
        }
        fn deliver(
            &mut self,
            _: &RemoteRegistration,
            _: &SignalTarget,
            _: Signal,
            _: &CleanupContext,
        ) -> Observed<()> {
            self.deliveries += 1;
            assert!(self.race, "unknown snapshot must never deliver");
            Observed::Unknown(inconsistent("member identity changed before delivery"))
        }
    }
    let cx = cx();
    let clean = cx.operation().cleanup(Duration::from_secs(1)).unwrap();
    let mut fake = FakeTransport {
        snapshot,
        deliveries: 0,
        race: false,
    };
    assert!(matches!(
        signal(&mut fake, &r, Signal::Term, &clean),
        Observed::Unknown(_)
    ));
    assert_eq!(fake.deliveries, 0);
    fake.snapshot.members.pop();
    fake.race = true;
    assert!(matches!(
        signal(&mut fake, &r, Signal::Kill, &clean),
        Observed::Unknown(_)
    ));
    assert_eq!(fake.deliveries, 1);
}

fn next() -> crate::outcome::Invocation {
    crate::outcome::Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap()
}
fn inv() -> ResolvedInvocation {
    let task = TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]),
        "",
        Surface::Published,
        "remote-test",
        None,
    )
    .unwrap();
    ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap()
}
#[test]
fn remote_fault_child() {
    let Ok(point) = std::env::var("EFFIGY_REMOTE_FAULT_CHILD") else {
        return;
    };
    let home = PathBuf::from(std::env::var_os("EFFIGY_REMOTE_FAULT_HOME").unwrap());
    let cx = cx();
    let root = StateRoot::effects(&cx, &home).unwrap();
    let mut a = Adapter(remote(launch_steps(&home, true)));
    let mut run = begin(&cx, &root, &mut a);
    let point = match point.as_str() {
        "identity" => crate::fault::Point::RemoteBeforeIdentity,
        "intent" => crate::fault::Point::RemoteAfterIntent,
        "client" => crate::fault::Point::SpawnAfterGo,
        "before-ack" => crate::fault::Point::RemoteBeforeAck,
        "ack" => crate::fault::Point::RemoteAfterAckBeforeRecord,
        "record" => crate::fault::Point::RemoteAfterRecord,
        _ => panic!("unknown fault"),
    };
    crate::fault::arm_point(point, crate::fault::Fault::Kill);
    let _ = RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut a.0);
    panic!("fault did not kill owner");
}
#[test]
fn owner_sigkill_at_each_remote_step_recovers_from_the_durable_journal() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    for point in [
        "identity",
        "intent",
        "client",
        "before-ack",
        "ack",
        "record",
    ] {
        let home = tempfile::tempdir().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "remote::tests::remote_fault_child",
                "--nocapture",
            ])
            .env("EFFIGY_REMOTE_FAULT_CHILD", point)
            .env("EFFIGY_REMOTE_FAULT_HOME", home.path())
            .env("HOME", home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("fault child exceeded deadline at {point}")
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.signal(), Some(libc::SIGKILL), "{point}: {status}");
        let cx = cx();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        // Serialize with the child's per-run sentinel before inspecting its
        // durable events. The sentinel has no engine adapter and retains residue.
        let gate = crate::gate::Gate::acquire(
            &cx,
            &root,
            &[inv().spec().owner.clone()],
            crate::gate::Wait::Until(Instant::now() + Duration::from_secs(5)),
        )
        .unwrap();
        let store = crate::store::Store::open(&cx, &root).unwrap();
        let row: Option<(String, Option<String>)> = {
            use rusqlite::OptionalExtension;
            store
                .conn()
                .query_row(
                    "SELECT target,receipt FROM events WHERE kind=?1",
                    [KIND],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .unwrap()
        };
        store.check_integrity().unwrap();
        drop(store);
        drop(gate);
        let mut steps = vec![];
        if let Some((spec, receipt)) = row {
            let spec: EffectSpec = serde_json::from_str(&spec).unwrap();
            let intent: Intent = serde_json::from_value(spec.payload).unwrap();
            let r = RemoteRegistration {
                nonce: intent.nonce.clone(),
                ..reg()
            };
            if let Some(receipt) = receipt {
                assert_eq!(
                    serde_json::from_str::<RemoteRegistration>(&receipt).unwrap(),
                    r
                );
            } else {
                steps.push(step(
                    RemoteCall::Find(container(), intent.nonce),
                    RemoteReply::Find(Observed::Verified(Some(r.clone()), evidence())),
                ));
            }
            steps.extend(closed_steps(&r));
        } else {
            assert_eq!(point, "identity");
        }
        let mut adapter = Adapter(remote(steps));
        let clean = cx.operation().cleanup(Duration::from_secs(5)).unwrap();
        let outcome = crate::run::settle_interrupted_wait(
            &cx,
            &root,
            (&inv().spec().owner, RunId::new(1, [1; 10]).unwrap()),
            next(),
            &clean,
            &mut adapter,
            crate::gate::Wait::Until(Instant::now() + Duration::from_secs(5)),
        )
        .unwrap();
        assert_eq!(outcome.cleanup(), CleanupOutcome::Complete, "{point}");
        adapter.0.tape.assert_finished();
    }
}
#[test]
fn empty_boot_group_only_absence_and_out_of_range_registration_cannot_close_or_signal() {
    use session::*;
    let r = reg();
    let empty = Snapshot {
        container: ContainerIdentity {
            vm_boot_id: String::new(),
            ..container()
        },
        pid_ns: 401,
        leader: None,
        members: vec![],
    };
    assert!(matches!(
        observe(&r, Observed::Verified(empty, evidence())),
        Observed::Unknown(_)
    ));
    assert!(!valid_absence(
        &r,
        &AbsenceProof::GroupEmpty { pgid: r.sid }
    ));
    let s = Snapshot {
        container: container(),
        pid_ns: 401,
        leader: None,
        members: vec![],
    };
    let mut bad = r.clone();
    bad.sid = 0;
    assert!(matches!(plan_signal(&bad, &s), Observed::Unknown(_)));
}
#[test]
fn remote_partial_effect_timeout_is_recorded_and_foreign_backend_is_untouched() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut steps = launch_steps(home.path(), true);
    steps[1].effects = vec![("owned-remote".into(), json!("partially-launched"))];
    steps[1].reply = RemoteReply::Launch(Box::new(|_| {
        Err(AdapterError(UnknownReason::TimedOut {
            phase: "remote launch".into(),
        }))
    }));
    let mut adapter = Adapter(remote(steps));
    let mut run = begin(&cx, &root, &mut adapter);
    let r = expected(&run);
    assert!(matches!(
        RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut adapter.0),
        Err(RemoteError::Unknown(UnknownReason::TimedOut { .. }))
    ));
    assert_eq!(
        adapter.0.tape.resources()["owned-remote"],
        json!("partially-launched")
    );
    adapter.0.tape.expect(step(
        RemoteCall::Find(container(), r.nonce.clone()),
        RemoteReply::Find(Observed::Verified(Some(r.clone()), evidence())),
    ));
    adapter.0.tape.expect(Step {
        call: RemoteCall::Observe(r),
        reply: session(SessionState::Closed),
        effects: vec![("owned-remote".into(), json!("closed"))],
    });
    let outcome = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
            &mut adapter,
        )
        .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert_eq!(adapter.0.tape.resources()["owned-remote"], json!("closed"));
    assert_eq!(adapter.0.tape.resources()["foreign"], json!("alive"));
    adapter.0.tape.assert_finished();
    let unselected = remote(vec![]);
    unselected.tape.assert_unselected();
}
#[test]
fn wrapper_is_posix_shell_preserves_argv_and_never_embeds_nonce_or_host_paths() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let cmd = RemoteCommand {
        argv: vec!["workload".into(), "space and ' quote $value".into()],
        env: BTreeMap::from([(
            "EFFIGY_CUSTODY_NONCE".into(),
            EnvValue::Secret(SecretValue::new(b"private-nonce-value".to_vec()).unwrap()),
        )]),
        terminal: RemoteTerminal::None,
    };
    let argv = cmd.wrapper_argv();
    assert_eq!(&argv[5..], &cmd.argv);
    assert!(argv
        .iter()
        .all(|a| !a.to_string_lossy().contains("private-nonce-value")));
    for script in [WRAPPER, REGISTER] {
        assert!(!script.contains("python"));
        assert!(!script.contains(".effigy"));
        assert!(!script.contains("touch "));
        let mut child = Command::new("/bin/sh")
            .arg("-n")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        assert!(child.wait_with_output().unwrap().status.success());
    }
}
#[test]
fn expired_phase_result_cannot_authorise_launch() {
    struct Late;
    impl RemoteExecAdapter for Late {
        fn identity(
            &mut self,
            _: &ContainerTarget,
            _: &OperationContext,
        ) -> Observed<ContainerIdentity> {
            std::thread::sleep(Duration::from_millis(10));
            Observed::Verified(container(), evidence())
        }
        fn launch(
            &mut self,
            _: &ContainerIdentity,
            _: RemoteCommand,
            _: &EffectContext,
        ) -> Result<ProcessSpec, AdapterError> {
            panic!("late identity must not launch")
        }
        fn signal_session(
            &mut self,
            _: &RemoteRegistration,
            _: Signal,
            _: &CleanupContext,
        ) -> Observed<SessionState> {
            panic!("unmodelled signal")
        }
        fn observe_session(
            &mut self,
            _: &RemoteRegistration,
            _: &OperationContext,
        ) -> Observed<SessionState> {
            panic!("unmodelled observe")
        }
        fn find_by_nonce(
            &mut self,
            _: &ContainerIdentity,
            _: &Nonce,
            _: &OperationContext,
        ) -> Observed<Option<RemoteRegistration>> {
            panic!("unmodelled find")
        }
    }
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut adapter = Adapter(remote(vec![]));
    let mut run = begin(&cx, &root, &mut adapter);
    let before = run.effects().unwrap().len();
    let mut spec = launch_spec();
    spec.call_timeout = Duration::from_millis(1);
    assert!(matches!(
        RemoteCustody::launch(&mut run, spec, &cx, &mut Late),
        Err(RemoteError::Unknown(UnknownReason::TimedOut { .. }))
    ));
    assert_eq!(run.effects().unwrap().len(), before);
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
        &mut adapter,
    )
    .unwrap();
}
#[cfg(target_os = "linux")]
#[test]
fn linux_wrapper_emits_real_namespace_and_start_identity_before_exec() {
    use std::process::Command;
    let r = reg();
    let command = RemoteCommand {
        argv: vec!["/bin/sh".into(), "-c".into(), "printf 'workload\\n'".into()],
        env: BTreeMap::new(),
        terminal: RemoteTerminal::None,
    };
    let argv = command.wrapper_argv();
    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .env("EFFIGY_CUSTODY_NONCE", r.nonce.as_str())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"workload\n");
    let boot = crate::ident::boot_identity()
        .verified()
        .unwrap()
        .as_str()
        .to_owned();
    let mut i = intent(&r);
    i.container.vm_boot_id = boot;
    let mut stream = stream::RegistrationStream::new(i, false);
    assert!(stream.push(&output.stderr, true).is_empty());
    let receipt = stream.registration().unwrap().unwrap();
    assert!(receipt.session_isolated);
    assert!(receipt.pid_ns > 0);
    assert!(receipt.leader_start_ticks > 0);
}
#[test]
fn killed_exec_client_leaves_remote_alive_until_independent_settlement() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut steps = launch_steps(home.path(), true);
    steps[1].reply = model_launch(home.path(), true, true);
    steps[1].effects = vec![("owned-remote".into(), json!("alive"))];
    let mut adapter = Adapter(remote(steps));
    let mut run = begin(&cx, &root, &mut adapter);
    let r = expected(&run);
    let mut remote = RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut adapter.0).unwrap();
    assert!(matches!(
        remote.client().forward(libc::SIGKILL),
        Observed::Verified(_, _)
    ));
    let killed = remote
        .client()
        .wait(&cx.operation().child(Duration::from_secs(1)));
    assert_ne!(killed.verified().unwrap().code, 0);
    assert_eq!(adapter.0.tape.resources()["owned-remote"], json!("alive"));
    assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
    let mut cleanup_steps = closed_steps(&r);
    cleanup_steps[4].effects = vec![("owned-remote".into(), json!("closed"))];
    for step in cleanup_steps {
        adapter.0.tape.expect(step)
    }
    let outcome = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
            &mut adapter,
        )
        .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert_eq!(adapter.0.tape.resources()["owned-remote"], json!("closed"));
    assert_eq!(adapter.0.tape.resources()["foreign"], json!("alive"));
    adapter.0.tape.assert_finished();
}
/// Another environment run: every bring-up advances the generation (053 §3).
fn advance_env(home: &Path) {
    let cx = cx();
    let root = StateRoot::effects(&cx, home).unwrap();
    let mut store = crate::store::Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| txn.next_generation(&env().resource).map(|_| ()))
        .unwrap();
}
#[test]
fn concurrent_environment_runs_leave_the_remote_outcome_truthful() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut steps = launch_steps(home.path(), true);
    let RemoteReply::Launch(launch) = std::mem::replace(
        &mut steps[1].reply,
        RemoteReply::Find(Observed::Unknown(UnknownReason::Injected)),
    ) else {
        unreachable!("launch step")
    };
    let bump = home.path().to_owned();
    // Between intent and acknowledgement.
    steps[1].reply = RemoteReply::Launch(Box::new(move |command| {
        advance_env(&bump);
        launch(command)
    }));
    let mut a = Adapter(remote(steps));
    let mut run = begin(&cx, &root, &mut a);
    let expected = expected(&run);
    let mut remote = RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut a.0).unwrap();
    // The registration keeps the generation bound at launch.
    assert_eq!(remote.registration, expected);
    // Between acknowledgement and completion.
    advance_env(home.path());
    let exit = *remote
        .client()
        .wait(&cx.operation().child(Duration::from_secs(1)))
        .verified()
        .unwrap();
    assert_eq!(exit.code, 0);
    remote.client().complete(&mut run, exit).unwrap();
    remote
        .complete(
            &mut run,
            crate::outcome::RequiredObligation::ProcessExit(exit.code),
        )
        .unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    a.0.tape.expect(step(
        RemoteCall::Observe(expected),
        session(SessionState::Closed),
    ));
    let outcome = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
            &mut a,
        )
        .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert_eq!(outcome.workload(), &WorkloadOutcome::Succeeded);
    a.0.tape.assert_finished();
}
#[test]
fn intent_still_binds_the_current_environment_generation() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut a = Adapter(remote(vec![step(
        RemoteCall::Identity(ContainerTarget("selected".into())),
        RemoteReply::Identity(Observed::Verified(container(), evidence())),
    )]));
    let mut run = begin(&cx, &root, &mut a);
    advance_env(home.path());
    let before = run.effects().unwrap().len();
    assert!(matches!(
        RemoteCustody::launch(&mut run, launch_spec(), &cx, &mut a.0),
        Err(RemoteError::Run(RunError::StaleGeneration { .. }))
    ));
    assert_eq!(run.effects().unwrap().len(), before);
    a.0.tape.assert_finished();
}
#[test]
fn a_replaced_container_or_rebooted_vm_is_refused_whatever_the_generation() {
    use session::*;
    let mut r = reg();
    // A registration bound before later environment runs.
    r.env.generation = Generation::new(1);
    let member = Member {
        pid: 42,
        start_ticks: 100,
        pgid: 42,
        sid: 42,
        nonce: Observed::Verified(r.nonce.as_str().to_owned(), evidence()),
        descendant: false,
    };
    let replaced = Snapshot {
        container: ContainerIdentity {
            id: "full-container-b".into(),
            ..container()
        },
        pid_ns: 401,
        leader: Some(member.clone()),
        members: vec![member.clone()],
    };
    assert!(matches!(plan_signal(&r, &replaced), Observed::Unknown(_)));
    assert!(matches!(
        observe(&r, Observed::Verified(replaced, evidence())),
        Observed::Unknown(_)
    ));
    let rebooted = Snapshot {
        container: ContainerIdentity {
            vm_boot_id: "boot-b".into(),
            ..container()
        },
        pid_ns: 401,
        leader: Some(member.clone()),
        members: vec![member],
    };
    assert!(matches!(plan_signal(&r, &rebooted), Observed::Unknown(_)));
    assert!(matches!(
        observe(&r, Observed::Verified(rebooted, evidence())),
        Observed::Absent(AbsenceProof::BootChanged { .. })
    ));
}
#[test]
fn nonce_recovery_binds_the_intent_fence_the_adapter_cannot_know() {
    let cx = cx();
    let clean = cx.operation().cleanup(Duration::from_secs(3)).unwrap();
    let r = reg();
    let found = RemoteRegistration {
        env: Fence {
            resource: r.env.resource.clone(),
            generation: Generation::new(9),
        },
        ..r.clone()
    };
    let mut steps = vec![step(
        RemoteCall::Find(container(), r.nonce.clone()),
        RemoteReply::Find(Observed::Verified(Some(found), evidence())),
    )];
    steps.extend(closed_steps(&r));
    let mut fake = remote(steps);
    assert_eq!(
        settle_effect(&effect(&r, false), &clean, Some(&mut fake)).0,
        EffectState::Released
    );
    fake.tape.assert_finished();
    // The identity is still the adapter's to prove.
    let mut fake = remote(vec![step(
        RemoteCall::Find(container(), r.nonce.clone()),
        RemoteReply::Find(Observed::Verified(
            Some(RemoteRegistration {
                container: ContainerIdentity {
                    id: "full-container-b".into(),
                    ..container()
                },
                ..r.clone()
            }),
            evidence(),
        )),
    )]);
    assert_eq!(
        settle_effect(&effect(&r, false), &clean, Some(&mut fake)).0,
        EffectState::Residue
    );
    fake.tape.assert_finished();
}
#[test]
fn a_backend_terminal_frames_on_stdout_with_its_line_ending() {
    let r = reg();
    let i = intent(&r);
    let frame = format!(
        "\x1eEFFIGY-CUSTODY v1 {} 42 42 100 boot-a 401\r\n",
        r.nonce.as_str()
    );
    let mut tty = stream::RegistrationStream::new(i.clone(), true);
    assert_eq!(tty.stream(), 0);
    let mut output = tty.push(frame.as_bytes(), false);
    output.extend(tty.push(b"prompt> ", true));
    assert_eq!(output, b"prompt> ");
    assert_eq!(tty.registration(), Some(Ok(r.clone())));
    let mut plain = stream::RegistrationStream::new(i, false);
    assert_eq!(plain.stream(), 1);
    assert!(plain.push(frame.as_bytes(), true).is_empty());
    assert!(plain.registration().unwrap().is_err());
}
#[test]
fn terminal_launch_inherits_stdin_and_strips_the_stdout_frame() {
    let home = tempfile::tempdir().unwrap();
    let cx = cx();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let cwd = home.path().to_owned();
    let mut a = Adapter(remote(vec![
        step(
            RemoteCall::Identity(ContainerTarget("selected".into())),
            RemoteReply::Identity(Observed::Verified(container(), evidence())),
        ),
        step(
            RemoteCall::Launch(container()),
            RemoteReply::Launch(Box::new(move |command| {
                assert_eq!(command.terminal, RemoteTerminal::Tty);
                let mut env = command.env;
                let nonce = env.remove(&OsString::from("EFFIGY_CUSTODY_NONCE")).unwrap();
                env.insert("TEST_REMOTE_NONCE".into(), nonce);
                Ok(ProcessSpec {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        "printf '\\036EFFIGY-CUSTODY v1 %s 42 42 100 boot-a 401\\r\\n' \"$TEST_REMOTE_NONCE\"; printf 'tty-output\\r\\n'".into(),
                    ],
                    cwd,
                    env,
                    grace: Duration::from_millis(20),
                    lifetime: crate::custody::Lifetime::Finite(Duration::from_secs(5)),
                    terminal: Terminal::None,
                    label: "remote fake tty exec".into(),
                })
            })),
        ),
    ]));
    let mut run = begin(&cx, &root, &mut a);
    let expected = expected(&run);
    let mut spec = launch_spec();
    spec.command.terminal = RemoteTerminal::Tty;
    let mut remote = RemoteCustody::launch(&mut run, spec, &cx, &mut a.0).unwrap();
    assert_eq!(remote.registration, expected);
    let exit = remote
        .client()
        .wait(&cx.operation().child(Duration::from_secs(1)));
    assert_eq!(exit.verified().unwrap().code, 0);
    let (stdout, stderr) = remote.client().take_stream_output();
    assert_eq!(stdout, b"tty-output\r\n");
    assert!(stderr.is_empty());
    a.0.tape.expect(step(
        RemoteCall::Observe(expected),
        session(SessionState::Closed),
    ));
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(3)).unwrap(),
        &mut a,
    )
    .unwrap();
    a.0.tape.assert_finished();
}
#[cfg(target_os = "linux")]
#[test]
fn linux_wrapper_keeps_an_existing_session_leader_instead_of_forking() {
    use std::process::Command;
    let r = reg();
    let command = RemoteCommand {
        argv: vec!["/bin/sh".into(), "-c".into(), "printf 'workload\\n'".into()],
        env: BTreeMap::new(),
        terminal: RemoteTerminal::Tty,
    };
    // A backend terminal makes the exec a session leader, as setsid(1) does.
    let child = Command::new("setsid")
        .args(command.wrapper_argv())
        .env("EFFIGY_CUSTODY_NONCE", r.nonce.as_str())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"workload\n");
    let mut i = intent(&r);
    i.container.vm_boot_id = crate::ident::boot_identity()
        .verified()
        .unwrap()
        .as_str()
        .to_owned();
    let mut stream = stream::RegistrationStream::new(i, false);
    assert!(stream.push(&output.stderr, true).is_empty());
    let receipt = stream.registration().unwrap().unwrap();
    // The process the client waits on is the registered leader.
    assert_eq!(receipt.leader_pid, pid);
    assert!(receipt.session_isolated);
}
