//! Real macOS/Linux custody qualification. All processes and files are fixtures.
use effigy_kernel::{
    ctx::*, custody::*, gate::Wait, id::*, observe::*, outcome::*, root::StateRoot, run::*,
    supervise::*,
};
use std::collections::BTreeMap;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
#[derive(Debug)]
struct Strict;
impl SettlementAdapter for Strict {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        panic!("unmodelled observation")
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        panic!("unmodelled release")
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        panic!("unmodelled reconcile")
    }
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn home() -> tempfile::TempDir {
    // Three characters at most: the fixture path plus a run id and `.sock`
    // must fit `sun_path`, and an Effigy run scratch root spends most of that
    // budget before the fixture name. A longer prefix fails the harness with
    // "cannot hold a ... fixture and its control socket".
    effigy_test_support::short_tempdir("kc-").unwrap()
}
fn fixture() -> PathBuf {
    std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_kernel-custody-fixture")))
}
fn context() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(20)),
        CancelToken::default(),
        Limits {
            capture_tail: 4096,
            log_rotate: 64,
            log_keep: 2,
            line_max: 1024,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Host,
    )
    .effects()
    .unwrap()
}
fn invocation() -> ResolvedInvocation {
    let task = TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [4; 16]),
        "",
        Surface::Published,
        "fixture",
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
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .unwrap()
}
fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap()
}
fn id(n: u8) -> RunId {
    RunId::new(1, [n; 10]).unwrap()
}
fn begin<'a>(cx: &EffectContext, root: &'a StateRoot) -> Run<'a> {
    Run::begin(
        cx,
        root,
        &invocation(),
        BeginOpts {
            id: id(1),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
        &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
        &mut Strict,
    )
    .unwrap()
}
fn workspace_session(cx: &EffectContext, root: &StateRoot) {
    let task = TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [4; 16]),
        "",
        Surface::Published,
        "workspace-session",
        None,
    )
    .unwrap();
    let mut spec = invocation().spec().clone();
    spec.owner = task.resource();
    spec.task = task;
    let run = Run::begin(
        cx,
        root,
        &ResolvedInvocation::new(spec).unwrap(),
        BeginOpts {
            id: id(7),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: None,
        },
        &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
        &mut Strict,
    )
    .unwrap();
    drop(run);
}
fn spec(home: &Path, script: &str) -> ProcessSpec {
    ProcessSpec {
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        cwd: home.into(),
        env: BTreeMap::new(),
        grace: Duration::from_millis(25),
        lifetime: Lifetime::Finite(Duration::from_secs(10)),
        terminal: Terminal::None,
        label: "fixture".into(),
    }
}
/// A poll is a new observation: StoreReader intentionally owns a snapshot.
fn fresh_status(cx: &ObserveContext, root: &StateRoot, id: RunId) -> Observed<RunStatus> {
    match effigy_kernel::store::Store::read(cx, root) {
        Observed::Verified(reader, _) => status(cx, root, &reader, id, next()),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}
fn eventually(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(12);
    while !predicate() {
        assert!(Instant::now() < end, "fixture deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn custody_prepare_source_requires_its_class_and_settles_with_a_sentinel() {
    for expected in [Some(0), Some(7), None] {
        let home = home();
        let mutate = context();
        let cx = OperationContext::new(
            Deadline::At(Instant::now() + Duration::from_secs(20)),
            CancelToken::default(),
            mutate.operation().limits().clone(),
            EffectClass::Prepare,
            effigy_kernel::ctx::Placement::Host,
        )
        .effects()
        .unwrap();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut coordinates = invocation().spec().clone();
        coordinates.class = EffectClass::Prepare;
        coordinates.owner = ResourceId::store(Scope::Machine, "bundles").unwrap();
        let mut run = Run::begin(
            &cx,
            &root,
            &ResolvedInvocation::new(coordinates).unwrap(),
            BeginOpts {
                id: id(1),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: Some(LifecyclePlan {
                    executable: Executable::capture(&fixture()).unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline,
                }),
            },
            &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
            &mut Strict,
        )
        .unwrap();
        assert!(matches!(
            Custody::spawn(&mut run, spec(home.path(), "exit 0"), &cx),
            Err(SpawnError::Refused(_))
        ));
        assert!(matches!(
            Custody::spawn_source(&mut run, spec(home.path(), "exit 0"), &mutate),
            Err(SpawnError::Refused(_))
        ));
        let script = match expected {
            Some(0) => "printf acquired",
            Some(_) => "exit 7",
            None => "sleep 60",
        };
        let mut process = spec(home.path(), script);
        if expected.is_none() {
            process.lifetime = Lifetime::Finite(Duration::from_millis(50));
        }
        let mut child = Custody::spawn_source(&mut run, process, &cx).unwrap();
        match (expected, child.wait(cx.operation())) {
            (Some(code), Observed::Verified(exit, _)) => {
                assert_eq!(exit.code, code);
                child.complete(&mut run, exit).unwrap();
                if code == 0 {
                    assert_eq!(child.take_output(), b"acquired");
                    run.finish(WorkloadOutcome::Succeeded).unwrap();
                } else {
                    run.finish(WorkloadOutcome::Failed { code }).unwrap();
                }
            }
            (None, Observed::Unknown(UnknownReason::TimedOut { .. })) => {
                run.interrupt(
                    &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
                    WorkloadOutcome::TimedOut {
                        phase: "source acquisition".into(),
                    },
                )
                .unwrap();
            }
            other => panic!("unexpected acquisition result: {other:?}"),
        }
        let result = run
            .settle(
                &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
                &mut Strict,
            )
            .unwrap();
        assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    }
}
#[test]
fn custody_partial_spawn_rolls_back_and_foreign_fixture_survives() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut foreign = ChildGuard(
        Command::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut bad = spec(home.path(), "exit 0");
    bad.argv[0] = "relative-forbidden".into();
    let failure = Custody::spawn_all(
        &mut run,
        vec![
            spec(home.path(), "trap '' TERM; /bin/sleep 30 & wait"),
            bad,
            spec(home.path(), "exit 0"),
        ],
        &cx,
    )
    .unwrap_err();
    assert_eq!(failure.cleanup.len(), 1);
    assert!(
        failure.cleanup[0].residue.is_empty(),
        "{:?}",
        failure.cleanup
    );
    assert!(foreign.0.try_wait().unwrap().is_none());
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
}
#[test]
fn custody_unreaped_leader_keeps_descendant_owned_until_stream_close() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut child = Custody::spawn(
        &mut run,
        spec(home.path(), "/bin/sh -c 'trap \"\" TERM; echo $$ > descendant; exec /bin/sleep 30' & while [ ! -f descendant ]; do :; done; exit 0"),
        &cx,
    )
    .unwrap();
    let receipt = child.identity();
    assert_eq!(
        child.wait(cx.operation()).verified(),
        Some(&Exit { code: 0 })
    );
    // Leader exit has not reaped or surrendered the group number.
    // macOS proc_pidinfo hides zombie identities. WNOWAIT is the direct proof
    // that this exact child is still ours and has not released its numeric pid.
    for _ in 0..2 {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::waitid(
                    libc::P_PID,
                    receipt.identity.pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            },
            0
        );
        assert_eq!(unsafe { info.si_pid() }, receipt.identity.pid as i32);
    }
    let report = child.stop(&cx.operation().cleanup(Duration::from_secs(5)).unwrap());
    assert!(report.residue.is_empty(), "{report:?}");
    assert_eq!(report.capture, CaptureState::Complete);
    let descendant: u32 = std::fs::read_to_string(home.path().join("descendant"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(!report.signals.is_empty());
    assert!(report
        .signals
        .iter()
        // Anchored custody signals freshly verified workload identities;
        // receipts describe those individual deliveries, including descendants.
        .all(|s| s.pid == descendant && !s.group));
    assert!(matches!(
        effigy_kernel::ident::process_identity(receipt.identity.pid),
        Observed::Absent(_)
    ));
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
}
#[test]
fn custody_secrets_stay_in_spawn_env_and_redact_split_chunks_before_all_sinks() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let secret = b"private-test-token-999";
    let mut process = spec(
        home.path(),
        "printf private-test-; /bin/sleep 0.05; printf token-999; /bin/sleep 0.1",
    );
    // The literal secret is split in argv, and is never a ps-visible argument.
    process.env.insert(
        "TOKEN".into(),
        EnvValue::Secret(SecretValue::new(secret.to_vec()).unwrap()),
    );
    let mut child = Custody::spawn(&mut run, process, &cx).unwrap();
    let args = Command::new("/bin/ps")
        .args([
            "-o",
            "args=",
            "-p",
            &child.identity().identity.pid.to_string(),
        ])
        .output()
        .unwrap();
    assert!(!args.stdout.windows(secret.len()).any(|w| w == secret));
    let exit = *child.wait(cx.operation()).verified().unwrap();
    child.complete(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(
        run.settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict
        )
        .unwrap()
        .workload(),
        &WorkloadOutcome::Succeeded
    );
    assert_eq!(child.tail(), b"[REDACTED]");
    let dir = root.path().join("runs").join(id(1).to_string());
    let logs = std::fs::read_dir(dir)
        .unwrap()
        .flat_map(|e| std::fs::read(e.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(logs, b"[REDACTED]");
}
#[test]
fn custody_secret_in_argv_is_refused_before_fork() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut process = spec(home.path(), "echo private999");
    process.env.insert(
        "TOKEN".into(),
        EnvValue::Secret(SecretValue::new(b"private999".to_vec()).unwrap()),
    );
    assert!(matches!(
        Custody::spawn(&mut run, process, &cx),
        Err(SpawnError::Refused(_))
    ));
    assert!(run.effects().unwrap().is_empty());
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict,
    )
    .unwrap();
}
#[test]
fn custody_signal_and_owner_kill_matrix_settles_without_survivors() {
    for placement in ["inline", "detached"] {
        for signal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM, libc::SIGKILL] {
            eprintln!("matrix {placement} signal={signal}");
            let home = home();
            let mut owner = ChildGuard(
                Command::new(fixture())
                    .arg(placement)
                    .arg(home.path())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            eventually(|| home.path().join("ready").exists());
            // Exact pid returned by our own spawn, never a pattern kill.
            assert_eq!(unsafe { libc::kill(owner.0.id() as i32, signal) }, 0);
            eventually(|| owner.0.try_wait().unwrap().is_some());
            let cx = context();
            let root = StateRoot::effects(&cx, home.path()).unwrap();
            let runid = RunId::new(1, [9; 10]).unwrap();
            eventually(
                || match fresh_status(&cx.operation().observe().unwrap(), &root, runid) {
                    Observed::Verified(RunStatus::Closed { complete: true, .. }, _) => true,
                    Observed::Verified(
                        RunStatus::Closed {
                            complete: false,
                            effects,
                            ..
                        },
                        _,
                    ) => panic!("{placement}/{signal}: incomplete {effects:?}"),
                    _ => false,
                },
            );
            let status = fresh_status(&cx.operation().observe().unwrap(), &root, runid);
            if let Observed::Verified(RunStatus::Closed { effects, .. }, _) = status {
                for effect in effects {
                    if effect.spec.kind != "kernel:process" {
                        continue;
                    }
                    if let Some(receipt) = effect.receipt {
                        let r: ProcessReceipt = serde_json::from_value(receipt).unwrap();
                        let result = unsafe { libc::kill(-(r.pgid as i32), 0) };
                        if result == 0 {
                            // only dead zombie members can briefly remain after reparenting
                            assert!(matches!(
                                effigy_kernel::ident::process_state(r.identity.pid),
                                Observed::Absent(_)
                                    | Observed::Verified(
                                        effigy_kernel::ident::ProcessState::Zombie,
                                        _
                                    )
                            ));
                        }
                    }
                }
            } else {
                panic!("not closed");
            }
        }
    }
}
#[test]
fn custody_detach_secondary_client_death_and_explicit_stop() {
    let home = home();
    let cx = context();
    let inv = invocation();
    let mut client = start_detached(
        &cx,
        home.path(),
        &inv,
        id(1),
        &next(),
        vec![spec(home.path(), "/bin/sleep 30")],
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    )
    .unwrap();
    while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let socket = root.path().join("sock").join(format!("{}.sock", id(1)));
    drop(std::os::unix::net::UnixStream::connect(&socket).unwrap());
    assert!(matches!(
        fresh_status(&cx.operation().observe().unwrap(), &root, id(1)),
        Observed::Verified(RunStatus::Live(_), _)
    ));
    let fence = client.fence.clone();
    assert_eq!(client.detach(&cx, id(2)).unwrap(), id(1));
    effigy_kernel::run::request(
        &cx,
        &root,
        &Request {
            id: id(3),
            run: id(1),
            expected: fence,
            action: RequestAction::Stop,
        },
    )
    .unwrap();
    eventually(|| {
        matches!(
            fresh_status(&cx.operation().observe().unwrap(), &root, id(1)),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
}
#[test]
fn custody_dead_sentinel_is_reported_as_a_gap_and_settles() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let run = begin(&cx, &root);
    let observe = cx.operation().observe().unwrap();
    let before = metadata(&observe, &root, id(1));
    let sentinel = before.verified().unwrap().sentinel.as_ref().unwrap();
    assert_eq!(
        before.verified().unwrap().sentinel_status,
        SentinelStatus::Live
    );
    assert_eq!(unsafe { libc::kill(sentinel.pid as i32, libc::SIGKILL) }, 0);
    eventually(|| {
        matches!(
            metadata(&observe, &root, id(1))
                .verified()
                .unwrap()
                .sentinel_status,
            SentinelStatus::Gap(_)
        )
    });
    run.settle(
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict,
    )
    .unwrap();
}

#[test]
fn custody_sentinel_recovers_nonce_descendant_after_leader_and_owner_die() {
    let home = home();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg("inline-orphan")
            .arg(home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.path().join("ready").exists());
    let descendant = std::fs::read_to_string(home.path().join("descendant-pid"))
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let identity = effigy_kernel::ident::process_identity(descendant)
        .verified()
        .cloned()
        .unwrap();
    assert_eq!(unsafe { libc::kill(owner.0.id() as i32, libc::SIGKILL) }, 0);
    eventually(|| owner.0.try_wait().unwrap().is_some());
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let observed = cx.operation().observe().unwrap();
    eventually(
        || match fresh_status(&observed, &root, RunId::new(1, [9; 10]).unwrap()) {
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _) => true,
            Observed::Verified(
                RunStatus::Closed {
                    complete: false,
                    effects,
                    ..
                },
                _,
            ) => panic!("{effects:?}"),
            _ => false,
        },
    );
    assert_ne!(
        effigy_kernel::ident::process_identity(descendant).verified(),
        Some(&identity)
    );
}

/// Q-014 (Tom 2026-10-27): a run that completes on its own leaves no anchor.
/// This is a direct hand-run invocation outside any suite marker, so settlement
/// alone retires the group evidence; nothing sweeps it afterwards.
#[test]
fn a_completed_invocation_leaves_no_custody_anchor() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut child = Custody::spawn(&mut run, spec(home.path(), "exit 0"), &cx).unwrap();
    let anchor = child.anchor().expect("the run is anchored");
    assert!(
        matches!(
            effigy_kernel::ident::process_state(anchor.pid),
            Observed::Verified(
                effigy_kernel::ident::ProcessState::Active
                    | effigy_kernel::ident::ProcessState::Stopped,
                _
            )
        ),
        "the anchor pins the group while the run is live"
    );
    let exit = *child.wait(cx.operation()).verified().unwrap();
    assert_eq!(exit.code, 0);
    child.complete(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    assert!(
        matches!(
            effigy_kernel::ident::process_state(anchor.pid),
            Observed::Absent(_) | Observed::Verified(effigy_kernel::ident::ProcessState::Zombie, _)
        ),
        "a settled run leaves no anchor"
    );
}

/// A lost anchor is not a live group. Q-014 makes TERM retire an idle anchor,/// so a completed workload can be left with no anchor at settle; an empty
/// original group still proves closure and settlement must finish.
#[test]
fn custody_settles_an_empty_group_after_its_anchor_exits() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut child = Custody::spawn(&mut run, spec(home.path(), "exit 0"), &cx).unwrap();
    let anchor = child.anchor().expect("the run is anchored");
    let exit = *child.wait(cx.operation()).verified().unwrap();
    assert_eq!(exit.code, 0);
    child.complete(&mut run, exit).unwrap();
    // Q-014: TERM retires an idle anchor. Wait for the recorded generation to
    // leave the live states, exactly as a cooperative exit does.
    assert!(matches!(
        effigy_kernel::custody::signal_verified_process(&anchor, libc::SIGTERM, || true),
        Observed::Verified(..)
    ));
    eventually(|| {
        !matches!(
            effigy_kernel::ident::process_state(anchor.pid),
            Observed::Verified(
                effigy_kernel::ident::ProcessState::Active
                    | effigy_kernel::ident::ProcessState::Stopped,
                _
            )
        )
    });
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
}

/// A live anchor still has to be its group's evidence. Its image is the run's
/// own executable, and an image that carries a file capability is a secure
/// exec: the kernel clears `dumpable`, and `/proc/<pid>/environ` refuses the
/// operator's reads. The installed `effigy` carries `cap_net_bind_service`, so
/// this is the Linux gateway-setup shape (effigy#476); the fixture models that
/// exec state for every anchor, which makes this proof the regression. On a
/// host without that kernel rule it still proves a live anchor retires last.
#[test]
fn custody_retires_an_anchor_whose_image_cleared_its_evidence() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut child = Custody::spawn(&mut run, spec(home.path(), "exit 0"), &cx).unwrap();
    let anchor = child.anchor().expect("the run is anchored");
    let exit = *child.wait(cx.operation()).verified().unwrap();
    assert_eq!(exit.code, 0);
    child.complete(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(20)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    assert!(
        matches!(
            effigy_kernel::ident::process_state(anchor.pid),
            Observed::Absent(_) | Observed::Verified(effigy_kernel::ident::ProcessState::Zombie, _)
        ),
        "settlement retires the anchor it was given"
    );
}

/// A sentinel settles a run whose owner died after its finite child exited and
/// its anchor was retired. The owner's cores are gone, so recovery proves the
/// original group from the kernel alone: an empty group is closure.
#[test]
fn custody_recovery_settles_an_empty_group_after_its_anchor_exits() {
    let home = home();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg("anchor-gone-orphan")
            .arg(home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.path().join("ready").exists());
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.path().join("anchor.json")).unwrap()).unwrap();
    let anchor: effigy_kernel::ident::ProcessIdentity =
        serde_json::from_value(record["anchor"].clone()).unwrap();
    assert!(matches!(
        effigy_kernel::custody::signal_verified_process(&anchor, libc::SIGTERM, || true),
        Observed::Verified(..)
    ));
    eventually(|| {
        !matches!(
            effigy_kernel::ident::process_state(anchor.pid),
            Observed::Verified(
                effigy_kernel::ident::ProcessState::Active
                    | effigy_kernel::ident::ProcessState::Stopped,
                _
            )
        )
    });
    assert_eq!(unsafe { libc::kill(owner.0.id() as i32, libc::SIGKILL) }, 0);
    eventually(|| owner.0.try_wait().unwrap().is_some());
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let observed = cx.operation().observe().unwrap();
    eventually(|| {
        match fresh_status(&observed, &root, RunId::new(1, [9; 10]).unwrap()) {
            Observed::Verified(RunStatus::Closed { effects, .. }, _) => {
                let residue = effects
                    .iter()
                    .filter(|effect| effect.reason.is_some())
                    .collect::<Vec<_>>();
                assert!(residue.is_empty(), "{residue:?}");
                true
            }
            _ => false,
        }
    });
}

#[test]
fn custody_pty_is_a_controlling_terminal_and_drains_before_reap() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut process = spec(
        home.path(),
        "test -t 0 && test -t 1 && test -t 2 && printf terminal-ok",
    );
    process.terminal = Terminal::Pty;
    let mut child = Custody::spawn(&mut run, process, &cx).unwrap();
    let exit = *child.wait(cx.operation()).verified().unwrap();
    assert_eq!(exit.code, 0);
    child.complete(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    assert_eq!(child.tail(), b"terminal-ok");
}

#[test]
fn custody_supervisor_kill_is_settled_and_stopped_owner_does_not_trigger_sentinel() {
    // An owner holds its pipe through SIGSTOP, unlike owner death.
    let home1 = home();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg("inline")
            .arg(home1.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    eventually(|| home1.path().join("ready").exists());
    let cx = context();
    let root1 = StateRoot::effects(&cx, home1.path()).unwrap();
    let observe = cx.operation().observe().unwrap();
    let runid = RunId::new(1, [9; 10]).unwrap();
    // SAFETY: exact PID of our own spawned fixture.
    assert_eq!(unsafe { libc::kill(owner.0.id() as i32, libc::SIGSTOP) }, 0);
    eventually(|| {
        matches!(
            effigy_kernel::ident::process_state(owner.0.id()),
            Observed::Verified(effigy_kernel::ident::ProcessState::Stopped, _)
        )
    });
    std::thread::sleep(Duration::from_millis(100));
    assert!(matches!(
        fresh_status(&observe, &root1, runid),
        Observed::Verified(RunStatus::Live(_), _)
    ));
    assert_eq!(
        metadata(&observe, &root1, runid)
            .verified()
            .unwrap()
            .sentinel_status,
        SentinelStatus::Live
    );
    // SAFETY: resume and stop only our own recorded fixture.
    assert_eq!(unsafe { libc::kill(owner.0.id() as i32, libc::SIGCONT) }, 0);
    assert_eq!(unsafe { libc::kill(owner.0.id() as i32, libc::SIGTERM) }, 0);
    eventually(|| owner.0.try_wait().unwrap().is_some());

    let home2 = home();
    let mut client = start_detached(
        &cx,
        home2.path(),
        &invocation(),
        id(1),
        &next(),
        vec![spec(home2.path(), "trap '' TERM; /bin/sleep 30 & wait")],
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    )
    .unwrap();
    while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
    let root2 = StateRoot::effects(&cx, home2.path()).unwrap();
    let info = metadata(&observe, &root2, id(1));
    let supervisor = info.verified().unwrap().supervisor.as_ref().unwrap();
    assert_eq!(
        effigy_kernel::ident::process_identity(supervisor.pid).verified(),
        Some(supervisor)
    );
    // SAFETY: identity-checked supervisor created by this client's start above.
    assert_eq!(
        unsafe { libc::kill(supervisor.pid as i32, libc::SIGKILL) },
        0
    );
    eventually(|| {
        matches!(
            fresh_status(&observe, &root2, id(1)),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
}

#[test]
fn custody_foreground_reads_terminal_and_restores_foreground_group() {
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    let home = home();
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: initialized writable slots; null terminal settings request defaults.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: openpty transferred these two new distinct descriptors.
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    // SAFETY: prevent the parent-side terminal master from leaking into the fixture.
    assert_eq!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
        0
    );
    let mut command = Command::new(fixture());
    command
        .arg("inline-terminal")
        .arg(home.path())
        .stdin(slave)
        .stdout(Stdio::null());
    // SAFETY: only session/terminal syscalls execute after fork, before exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut owner = ChildGuard(command.spawn().unwrap());
    eventually(|| home.path().join("ready").exists());
    master.write_all(b"terminal-input\n").unwrap();
    eventually(|| home.path().join("terminal-restored").exists());
    // macOS waits for its PTY peer during session-leader exit. Restoration
    // is already proven; close our master before waiting for that exit.
    drop(master);
    eventually(|| owner.0.try_wait().unwrap().is_some());
}

#[test]
fn custody_sequential_foreground_spawns_keep_the_owner_in_the_foreground() {
    sequential_foreground_spawns(false);
}

#[test]
fn custody_sequential_foreground_spawns_survive_tostop() {
    sequential_foreground_spawns(true);
}

/// Two sequential `Terminal::Foreground` children in one owner run: the first
/// writes to the terminal and exits, the second then reads a line from it.
/// A `TCSETPGRP` from a background owner stops the owner with `SIGTTOU`, and
/// `TOSTOP` adds a stop on any background terminal write.
fn sequential_foreground_spawns(tostop: bool) {
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    let home = home();
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: initialized writable slots; null terminal settings request defaults.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: openpty transferred two fresh distinct descriptors.
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    // SAFETY: live master; observe it nonblocking and never inherit it.
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFL);
        assert_eq!(
            libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
            0
        );
        assert_eq!(
            libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC),
            0
        );
    }
    if tostop {
        // SAFETY: the live terminal's own settings; TOSTOP stops a background writer.
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            assert_eq!(libc::tcgetattr(slave.as_raw_fd(), &mut termios), 0);
            termios.c_lflag |= libc::TOSTOP;
            assert_eq!(
                libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &termios),
                0
            );
        }
    }
    let mut command = Command::new(fixture());
    command
        .arg("inline-terminal-sequence")
        .arg(home.path())
        .stdin(slave)
        .stdout(Stdio::null());
    // SAFETY: only session and terminal syscalls run after fork, before exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut owner = ChildGuard(command.spawn().unwrap());
    let pid = owner.0.id();
    // The first child wrote to the terminal and its lease ended with its exit.
    let mut seen = Vec::new();
    eventually(|| {
        assert_running(pid);
        drain_terminal(&master, &mut seen);
        home.path().join("first-done").exists()
    });
    eventually(|| {
        assert_running(pid);
        drain_terminal(&master, &mut seen);
        seen.windows(8).any(|window| window == b"first-ok")
    });
    assert_eq!(foreground_group(&master), pid as i32);
    std::fs::write(home.path().join("first-observed"), b"yes").unwrap();
    master.write_all(b"terminal-input\n").unwrap();
    eventually(|| {
        assert_running(pid);
        drain_terminal(&master, &mut seen);
        home.path().join("second-done").exists()
    });
    assert_eq!(foreground_group(&master), pid as i32);
    std::fs::write(home.path().join("second-observed"), b"yes").unwrap();
    eventually(|| home.path().join("sequence-done").exists());
    // macOS waits for its PTY peer during session-leader exit.
    drop(master);
    eventually(|| owner.0.try_wait().unwrap().is_some());
    let status = owner.0.wait().unwrap();
    assert!(status.success(), "terminal sequence failed: {status:?}");
}

fn drain_terminal(master: &std::fs::File, into: &mut Vec<u8>) {
    let mut buffer = [0u8; 256];
    loop {
        // SAFETY: live nonblocking descriptor and an initialized buffer.
        let count =
            unsafe { libc::read(master.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if count > 0 {
            into.extend_from_slice(&buffer[..count as usize]);
        } else {
            return;
        }
    }
}

fn foreground_group(master: &std::fs::File) -> i32 {
    // SAFETY: the live terminal's own foreground group.
    unsafe { libc::tcgetpgrp(master.as_raw_fd()) }
}

/// The owner of a real terminal never gets stopped: a stopped owner blocks its
/// own sequence forever, which is exactly the reported suspension.
fn assert_running(pid: u32) {
    if let Observed::Verified(state, _) = effigy_kernel::ident::process_state(pid) {
        assert_ne!(
            state,
            effigy_kernel::ident::ProcessState::Stopped,
            "owner {pid} was stopped by a terminal signal"
        );
    }
}

#[test]
fn custody_detached_escalation_interrupts_an_in_progress_settle_grace() {
    let home = home();
    let cx = context();
    let mut process = spec(
        home.path(),
        "trap '' TERM; printf ready > workload-ready; /bin/sleep 30 & wait",
    );
    process.grace = Duration::from_secs(10);
    let mut client = start_detached(
        &cx,
        home.path(),
        &invocation(),
        id(1),
        &next(),
        vec![process],
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    )
    .unwrap();
    while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
    eventually(|| home.path().join("workload-ready").exists());
    client.stop(&cx, id(2)).unwrap();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let observe = cx.operation().observe().unwrap();
    eventually(|| {
        matches!(
            fresh_status(&observe, &root, id(1)),
            Observed::Verified(RunStatus::Live(Phase::Stopping), _)
        )
    });
    let at = Instant::now();
    client.escalate(&cx, id(3)).unwrap();
    eventually(|| {
        matches!(
            fresh_status(&observe, &root, id(1)),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
    assert!(
        at.elapsed() < Duration::from_secs(2),
        "escalation waited for the ten-second grace"
    );
}

#[test]
fn custody_terminal_close_stops_detached_owning_client() {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    let home = home();
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: initialized writable slots; OS-default terminal configuration.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: two distinct fresh descriptors from successful openpty.
    let master = unsafe { std::fs::File::from_raw_fd(master) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    // SAFETY: live master; forbid inheritance across the fixture exec.
    assert_eq!(
        unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) },
        0
    );
    let mut command = Command::new(fixture());
    command
        .arg("detached")
        .arg(home.path())
        .stdin(slave)
        .stdout(Stdio::null());
    // SAFETY: only async-signal-safe session and terminal operations after fork.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut client = ChildGuard(command.spawn().unwrap());
    eventually(|| home.path().join("ready").exists());
    drop(master); // A real terminal hangup, rather than an injected HUP alone.
    eventually(|| client.0.try_wait().unwrap().is_some());
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let observe = cx.operation().observe().unwrap();
    eventually(|| {
        matches!(
            fresh_status(&observe, &root, RunId::new(1, [9; 10]).unwrap()),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
}

#[test]
fn custody_both_owner_and_sentinel_die_successor_is_the_only_settle_writer() {
    let home = home();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg("inline")
            .arg(home.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.path().join("ready").exists());
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let observe = cx.operation().observe().unwrap();
    let runid = RunId::new(1, [9; 10]).unwrap();
    let info = metadata(&observe, &root, runid);
    let sentinel = info.verified().unwrap().sentinel.as_ref().unwrap();
    assert_eq!(
        effigy_kernel::ident::process_identity(sentinel.pid).verified(),
        Some(sentinel)
    );
    // SAFETY: the identity-checked sentinel and exact owner spawned by this proof.
    assert_eq!(unsafe { libc::kill(sentinel.pid as i32, libc::SIGKILL) }, 0);
    eventually(|| {
        matches!(
            metadata(&observe, &root, runid)
                .verified()
                .unwrap()
                .sentinel_status,
            SentinelStatus::Gap(_)
        )
    });
    assert_eq!(unsafe { libc::kill(owner.0.id() as i32, libc::SIGKILL) }, 0);
    eventually(|| owner.0.try_wait().unwrap().is_some());
    assert!(matches!(
        fresh_status(&observe, &root, runid),
        Observed::Verified(RunStatus::Interrupted { .. }, _)
    ));
    let outcome = settle_interrupted(
        &cx,
        &root,
        &invocation().spec().owner,
        runid,
        next(),
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict,
    )
    .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert_eq!(outcome.workload(), &WorkloadOutcome::Interrupted);
}

#[test]
fn custody_merged_streams_cannot_reconstruct_a_secret_before_any_sink() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = begin(&cx, &root);
    let mut process = spec(home.path(), "printf private; printf 999 >&2");
    process.env.insert(
        "TOKEN".into(),
        EnvValue::Secret(SecretValue::new(b"private999".to_vec()).unwrap()),
    );
    let mut child = Custody::spawn(&mut run, process, &cx).unwrap();
    let exit = *child.wait(cx.operation()).verified().unwrap();
    child.complete(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let result = run
        .settle(
            &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
            &mut Strict,
        )
        .unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    assert_eq!(child.tail(), b"[REDACTED]");
    assert_eq!(child.take_output(), b"[REDACTED]");
    // The per-stream queues and logs come from the same merged pass: joined
    // in any order they never rebuild the secret.
    let (stdout, stderr) = child.take_stream_output();
    assert_eq!([stdout, stderr].concat(), b"[REDACTED]");
    let logs = std::fs::read_dir(root.path().join("runs").join(id(1).to_string()))
        .unwrap()
        .flat_map(|e| std::fs::read(e.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(logs, b"[REDACTED]");
}

/// An engine client process, killed or exiting by itself; the test reaps it.
fn engine_client(home: &Path, mode: &str) -> ChildGuard {
    let client = ChildGuard(
        Command::new(fixture())
            .arg("engine-client")
            .arg(home)
            .arg(mode)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.join("client-ready").exists());
    client
}
fn engine_status(home: &Path) -> Observed<RunStatus> {
    let cx = context();
    let root = StateRoot::effects(&cx, home).unwrap();
    let observe = cx.operation().observe().unwrap();
    fresh_status(&observe, &root, RunId::new(1, [9; 10]).unwrap())
}
fn workload_gone(home: &Path) -> bool {
    let pid: i32 = std::fs::read_to_string(home.join("engine-workload.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // SAFETY: probe-only signal 0 to the fixture workload this test recorded.
    unsafe { libc::kill(pid, 0) != 0 }
}
fn engine_output(home: &Path) -> String {
    let cx = context();
    let root = StateRoot::effects(&cx, home).unwrap();
    let observe = cx.operation().observe().unwrap();
    let read = read_log(
        &observe,
        &root,
        RunId::new(1, [9; 10]).unwrap(),
        OUTPUT_SEQUENCE,
        Stream::Stdout,
        0,
        4096,
    );
    String::from_utf8(read.verified().unwrap().bytes.clone()).unwrap()
}

/// Ruling 5: an owning client's death, here an agent's SIGKILL, stops and
/// settles the engine's run; its workload does not survive (O1a).
#[test]
fn custody_engine_owning_client_kill_stops_and_settles() {
    let home = home();
    let mut client = engine_client(home.path(), "owning");
    assert!(matches!(
        engine_status(home.path()),
        Observed::Verified(RunStatus::Live(_), _)
    ));
    // SAFETY: SIGKILL to the fixture client process this test spawned.
    assert_eq!(
        unsafe { libc::kill(client.0.id() as i32, libc::SIGKILL) },
        0
    );
    client.0.wait().unwrap();
    eventually(|| {
        matches!(
            engine_status(home.path()),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
    let Observed::Verified(RunStatus::Closed { workload, .. }, _) = engine_status(home.path())
    else {
        panic!("closed");
    };
    assert!(
        matches!(
            workload,
            WorkloadOutcome::Cancelled {
                cause: StopCause::OwnerLost,
                ..
            }
        ),
        "{workload:?}"
    );
    eventually(|| workload_gone(home.path()));
    // The engine got its client's cwd and explicit environment, and its own
    // output reached the run log redacted. The journal closes before the
    // engine flushes its capture, so wait for the tail.
    eventually(|| engine_output(home.path()).ends_with('\n'));
    let output = engine_output(home.path());
    assert!(output.contains("engine output [REDACTED]"), "{output:?}");
    assert!(output.contains("env=present"), "{output:?}");
    assert!(
        output.contains(&format!("cwd={}", home.path().display())),
        "{output:?}"
    );
}

/// A detached client exits without stopping the run; a stop request then
/// settles it through the same owner loop.
#[test]
fn custody_engine_detached_client_exit_keeps_the_run_until_stop() {
    let home = home();
    let mut client = engine_client(home.path(), "detach");
    eventually(|| client.0.try_wait().unwrap().is_some());
    std::thread::sleep(Duration::from_millis(300));
    assert!(matches!(
        engine_status(home.path()),
        Observed::Verified(RunStatus::Live(_), _)
    ));
    assert!(!workload_gone(home.path()));
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let observe = cx.operation().observe().unwrap();
    let reader = effigy_kernel::store::Store::read(&observe, &root);
    let summary = summary(reader.verified().unwrap(), RunId::new(1, [9; 10]).unwrap());
    let summary = summary.verified().unwrap();
    request(
        &cx,
        &root,
        &Request {
            id: id(7),
            run: summary.id,
            expected: Fence {
                resource: invocation().spec().owner.clone(),
                generation: summary.generation,
            },
            action: RequestAction::Stop,
        },
    )
    .unwrap();
    eventually(|| {
        matches!(
            engine_status(home.path()),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
    assert_eq!(
        std::fs::read_to_string(home.path().join("engine-stop-cause")).unwrap(),
        "Requested"
    );
    assert!(workload_gone(home.path()));
}

/// Owning-client frames reach the engine, its events reach the client, and
/// a second start on the same owner is refused while the first is live.
#[test]
fn custody_engine_frames_round_trip_and_a_second_start_is_refused() {
    let home = home();
    let cx = context();
    // 052 §8.11: a host dispatcher reserves the keyed request and hands the
    // ticket to the detached supervisor, whose engine admits under it.
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    workspace_session(&cx, &root);
    let key = AdmissionKey {
        session: Session {
            run: id(7),
            generation: Generation::new(1),
        },
        request: RequestId::new([3; 16]),
    };
    let digest = Digest::of(b"effigy.custody.engine.fixture.v1");
    let ticket = match reserve(&cx, &root, key, digest.clone(), id(9)).unwrap() {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("expected the one reservation, saw {other:?}"),
    };
    let mut client = start_engine(
        &cx,
        home.path(),
        EngineLaunch {
            payload: home.path().as_os_str().as_encoded_bytes().to_vec(),
            cwd: home.path().into(),
            env: vec![],
            handoff: None,
            admission: Some(ticket),
        },
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    )
    .unwrap();
    assert_eq!(client.run, RunId::new(1, [9; 10]).unwrap());
    // The detached engine's admission bound the key, in its own process.
    let _reader = match effigy_kernel::store::Store::read(&cx.operation().observe().unwrap(), &root)
    {
        Observed::Verified(reader, _) => reader,
        other => panic!("store reader {other:?}"),
    };
    match lookup(
        &cx.operation().observe().unwrap(),
        &root,
        key,
        digest.clone(),
    ) {
        Observed::Verified(Recovered::Admitted { run, .. }, _) => {
            assert_eq!(run, RunId::new(1, [9; 10]).unwrap())
        }
        other => panic!("detached engine admission {other:?}"),
    }
    while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
    client.send_frame(b"hello engine").unwrap();
    loop {
        if let Event::Engine(bytes) = client.event(cx.operation()).unwrap() {
            assert_eq!(bytes, b"hello engine");
            break;
        }
    }
    let second = start_engine(
        &cx,
        home.path(),
        EngineLaunch {
            payload: home.path().as_os_str().as_encoded_bytes().to_vec(),
            cwd: home.path().into(),
            env: vec![],
            handoff: None,
            admission: None,
        },
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    );
    assert!(second.is_err(), "a second engine run was admitted");
    client.stop(&cx, id(8)).unwrap();
    client.wait(cx.operation()).unwrap();
    assert!(matches!(
        engine_status(home.path()),
        Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
    ));
}

/// Attaching caller dies after row commit, before returning its response to
/// the parent or registering a supervisor watch. The unit fault proof kills
/// at RunAttachAfter, before even the handle is returned.
#[test]
#[ignore = "helper process"]
fn helper_workspace_secondary_creation_loss() {
    let home = std::env::var_os("EFFIGY_CUSTODY_ATTACHMENT_HOME").expect("private fixture root");
    let cx = context();
    let root = StateRoot::effects(&cx, Path::new(&home)).unwrap();
    let _ = attach_secondary(
        &cx,
        &root,
        Session {
            run: id(7),
            generation: Generation::new(1),
        },
        id(9),
        AttachmentId::new([8; 16]),
    )
    .expect("secondary commit");
    // SAFETY: kill only this explicitly spawned helper, before handing the
    // committed attachment back to the parent or to any liveness watcher.
    assert_eq!(
        unsafe { libc::kill(std::process::id() as libc::pid_t, libc::SIGKILL) },
        0
    );
    panic!("self SIGKILL returned");
}

/// Handle EOF and acknowledged detach drive the real detached engine's custody,
/// including a killed secondary's independent liveness pipe and slot reuse.
#[test]
fn custody_workspace_handles_drive_owner_and_secondary_lifetimes() {
    for (cause, collision) in [
        (Release::Eof, Some("foreign")),
        (Release::Eof, Some("resolved")),
        (Release::Detach, None),
    ] {
        let home = home();
        eprintln!(
            "workspace lifetime {cause:?}: setup {}",
            home.path().display()
        );
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        workspace_session(&cx, &root);
        let key = AdmissionKey {
            session: Session {
                run: id(7),
                generation: Generation::new(1),
            },
            request: RequestId::new([4; 16]),
        };
        let digest = Digest::of(b"workspace-lifetimes");
        let ticket = match reserve(&cx, &root, key, digest.clone(), id(9)).unwrap() {
            Reservation::Fresh(ticket) => ticket,
            other => panic!("reservation {other:?}"),
        };
        let mut client = start_engine(
            &cx,
            home.path(),
            EngineLaunch {
                payload: home.path().as_os_str().as_encoded_bytes().to_vec(),
                cwd: home.path().into(),
                env: vec![],
                handoff: None,
                admission: Some(ticket),
            },
            &Executable::capture(&fixture()).unwrap(),
            ClientMode::Owning,
        )
        .unwrap();
        eprintln!("workspace lifetime {cause:?}: waiting for Active");
        while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
        let cx = context();

        eprintln!("workspace lifetime {cause:?}: losing secondary creation response");
        let mut attaching = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "helper_workspace_secondary_creation_loss",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                ])
                .env("EFFIGY_CUSTODY_ATTACHMENT_HOME", home.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        eventually(|| match attaching.0.try_wait().unwrap() {
            Some(status) => {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(status.signal(), Some(libc::SIGKILL));
                true
            }
            None => false,
        });
        let secondary_id = AttachmentId::new([8; 16]);
        // Creation committed, but the owner has never seen a handle or watch.
        assert!(attach_secondary(&cx, &root, key.session, id(9), secondary_id).is_err());
        assert!(
            resume_secondary(&cx, &root, key.session, id(9), secondary_id)
                .unwrap()
                .attached(&cx, &root)
                .unwrap()
        );
        assert!(matches!(
            engine_status(home.path()),
            Observed::Verified(RunStatus::Live(_), _)
        ));
        let listener = std::os::unix::net::UnixListener::bind(home.path().join("a.sock")).unwrap();
        listener.set_nonblocking(true).unwrap();
        eprintln!("workspace lifetime {cause:?}: requesting secondary watch");
        client.send_frame(b"resume-secondary").unwrap();
        loop {
            if let Event::Engine(bytes) = client.event(cx.operation()).unwrap() {
                assert_eq!(
                    String::from_utf8(bytes).unwrap(),
                    "resume-secondary",
                    "recovered secondary watch acknowledgement"
                );
                break;
            }
        }
        eprintln!("workspace lifetime {cause:?}: accepting secondary socket");
        let accept_deadline = Instant::now() + Duration::from_secs(10);
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < accept_deadline,
                        "secondary socket deadline"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("secondary socket: {error}"),
            }
        };
        let mut terminal = ChildGuard(
            Command::new("/bin/sleep")
                .arg("40")
                .stdin(Stdio::from(std::os::fd::OwnedFd::from(socket)))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        eprintln!("workspace lifetime {cause:?}: killing secondary terminal");
        terminal.0.kill().unwrap();
        eventually(|| match terminal.0.try_wait().unwrap() {
            Some(status) => {
                assert!(!status.success());
                true
            }
            None => false,
        });
        eprintln!("workspace lifetime {cause:?}: waiting for secondary release");
        eventually(|| {
            // The supervisor records only the secondary's EOF; the owner lives.
            match effigy_kernel::store::Store::read(&cx.operation().observe().unwrap(), &root) {
                Observed::Verified(_reader, _) => rusqlite::Connection::open_with_flags(root.path().join("kernel.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap().query_row(
                    "SELECT count(*) FROM run_attachments WHERE lifetime='secondary' AND state='attached'", [], |r| r.get::<_, i64>(0)
                ).unwrap() == 0,
                _ => false,
            }
        });
        assert!(matches!(
            engine_status(home.path()),
            Observed::Verified(RunStatus::Live(_), _)
        ));
        eprintln!("workspace lifetime {cause:?}: refilling secondary slots");
        let cx = context();
        // All eight slots can be filled after the killed terminal is released.
        for n in 0..SECONDARY_MAX {
            attach_secondary(
                &cx,
                &root,
                key.session,
                id(9),
                AttachmentId::new([20 + n as u8; 16]),
            )
            .unwrap();
        }
        let mut owner = resume_owner(&cx, &root, key.session, id(9)).unwrap();
        if let Some(collision) = collision {
            let mut material = b"effigy.workspace.owner-eof-stop.v1".to_vec();
            material.extend_from_slice(owner.id().bytes());
            let digest = Digest::of(&material);
            let mut entropy = [0u8; 10];
            entropy.copy_from_slice(&digest.0[..10]);
            let request_id = RunId::new(0, entropy).unwrap();
            let foreign = collision == "foreign";
            let request = Request {
                id: request_id,
                run: if foreign { id(7) } else { id(9) },
                expected: owner.fence().clone(),
                action: RequestAction::Stop,
            };
            let conn = rusqlite::Connection::open(root.path().join("kernel.db")).unwrap();
            conn.execute("INSERT INTO requests(request_id,run_id,expected_generation,action,state,created_at) VALUES (?1,?2,1,?3,?4,0)", rusqlite::params![request_id.to_string(), request.run.to_string(), serde_json::to_string(&request).unwrap(), if foreign {"pending"} else {"resolved"}]).unwrap();
        }
        eprintln!("workspace lifetime {cause:?}: releasing owner");
        owner.release(&cx, &root, cause).unwrap();
        if cause == Release::Detach {
            // A round trip proves the supervisor consumed the handle's detach
            // before EOF; a delayed control turn cannot make this proof pass.
            client.send_frame(b"confirm-detached").unwrap();
            loop {
                if let Event::Engine(bytes) = client.event(cx.operation()).unwrap() {
                    assert_eq!(bytes, [1]);
                    break;
                }
            }
            // Closing the original owning pipe must respect handle detach.
            // Keep the Child handle so this test still reaps its own supervisor.
            client.close_input();
            std::thread::sleep(Duration::from_millis(150));
            assert!(matches!(
                engine_status(home.path()),
                Observed::Verified(RunStatus::Live(_), _)
            ));
            client.stop(&cx, id(8)).unwrap();
        }
        eprintln!("workspace lifetime {cause:?}: waiting for supervisor settlement");
        client.wait(cx.operation()).unwrap();
        eprintln!("workspace lifetime {cause:?}: verifying settlement");
        assert!(workload_gone(home.path()));
        let _reader =
            match effigy_kernel::store::Store::read(&cx.operation().observe().unwrap(), &root) {
                Observed::Verified(reader, _) => reader,
                other => panic!("store {other:?}"),
            };
        assert!(matches!(
            lookup(&cx.operation().observe().unwrap(), &root, key, digest),
            Observed::Verified(
                Recovered::Completed {
                    outcome: RecordedOutcome {
                        workload: WorkloadOutcome::Cancelled {
                            cause: StopCause::Requested,
                            ..
                        },
                        cleanup_complete: true
                    },
                    ..
                },
                _
            )
        ));
        assert_eq!(
            rusqlite::Connection::open_with_flags(
                root.path().join("kernel.db"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            )
            .unwrap()
            .query_row("SELECT count(*) FROM run_attachments", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
            0
        );
    }
}

fn foreground_begin(cx: &EffectContext, root: &StateRoot) -> ForegroundRun {
    ForegroundRun::begin(
        cx,
        root,
        &invocation(),
        BeginOpts {
            id: id(1),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
    )
    .unwrap()
}

#[test]
fn foreground_custody_completes_without_database_or_logs() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    for _ in 0..2 {
        let mut run = foreground_begin(&cx, &root);
        let mut child =
            Custody::spawn_foreground(&mut run, spec(home.path(), "printf hello"), &cx).unwrap();
        let exit = child.wait(cx.operation()).verified().copied().unwrap();
        assert_eq!(exit.code, 0);
        child.complete_foreground(&mut run, exit).unwrap();
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        let outcome = run
            .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
            .unwrap();
        assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
        assert_eq!(outcome.workload(), &WorkloadOutcome::Succeeded);
        assert!(!root.path().join("kernel.db").exists());
        assert!(!root.path().join("runs").exists());
        assert!(!root.path().join("sock").exists());
        assert!(!root.path().join("objects").exists());
        assert!(matches!(
            effigy_kernel::gate::Gate::probe(
                &cx.operation().observe().unwrap(),
                &root,
                &invocation().spec().owner
            ),
            Observed::Verified(effigy_kernel::gate::GateState::Free, _)
        ));
    }
}

/// A sibling checkout journal written throughout is not an obstacle: the
/// foreground read is one plain WAL read, and the run starts in bounded time
/// without busy-waiting (K1).
#[test]
fn foreground_begins_beside_a_journal_written_throughout() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut store = effigy_kernel::store::Store::open(&cx, &root).unwrap();
    store
        .txn(&cx, |txn| {
            txn.resource(&invocation().spec().owner, "fixture")
                .map(|_| ())
        })
        .unwrap();
    drop(store);
    let path = root.path().join(effigy_kernel::store::DB_NAME);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writing = stop.clone();
    let writer = std::thread::spawn(move || {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection.busy_timeout(Duration::from_secs(5)).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE churn (id INTEGER PRIMARY KEY, blob BLOB);\
                 INSERT INTO churn(id, blob) \
                 WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 64) \
                 SELECT x, randomblob(4096) FROM c;",
            )
            .unwrap();
        while !writing.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = connection.execute(
                "UPDATE churn SET blob = randomblob(4096) WHERE id = (abs(random()) % 64) + 1",
                [],
            );
        }
    });
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    let mut run = foreground_begin(&cx, &root);
    let mut child =
        Custody::spawn_foreground(&mut run, spec(home.path(), "printf hello"), &cx).unwrap();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0);
    child.complete_foreground(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let outcome = run
        .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
        .unwrap();
    let elapsed = started.elapsed();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    writer.join().unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert!(
        elapsed < Duration::from_secs(1),
        "a foreground run beside a writer took {elapsed:?}"
    );
}

/// A journal that is not a store warns and the run proceeds; nothing is
/// repaired behind the operator's back (K1, Q-011).
#[test]
fn foreground_warns_and_runs_with_a_damaged_journal() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    effigy_kernel::store::Store::open(&cx, &root).unwrap();
    let journal = root.path().join(effigy_kernel::store::DB_NAME);
    std::fs::write(&journal, b"not a store").unwrap();
    let mut run = foreground_begin(&cx, &root);
    let mut child =
        Custody::spawn_foreground(&mut run, spec(home.path(), "printf hello"), &cx).unwrap();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0);
    child.complete_foreground(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let outcome = run
        .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
        .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert_eq!(std::fs::read(&journal).unwrap(), b"not a store".to_vec());
    std::fs::remove_file(&journal).unwrap();
}

#[test]
fn foreground_leader_exit_keeps_descendant_custody_and_cleans_on_drop() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = foreground_begin(&cx, &root);
    let mut child = Custody::spawn_foreground(
        &mut run,
        spec(home.path(), "/bin/sleep 30 >/dev/null 2>&1 & exit 0"),
        &cx,
    )
    .unwrap();
    let receipt = child.identity();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0);
    child.complete_foreground(&mut run, exit).unwrap();
    drop(run);
    assert!(matches!(
        effigy_kernel::gate::Gate::probe(
            &cx.operation().observe().unwrap(),
            &root,
            &invocation().spec().owner
        ),
        Observed::Verified(effigy_kernel::gate::GateState::Free, _)
    ));
    // Numeric reuse is not authority: the observable group must be gone.
    let output = Command::new("/bin/ps")
        .args(["-axo", "pgid="])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim() == receipt.pgid.to_string()));
    assert!(!root.path().join("kernel.db").exists());
}

#[test]
fn foreground_guard_death_refuses_new_go_but_owner_closes_existing_custody() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = foreground_begin(&cx, &root);
    let child =
        Custody::spawn_foreground(&mut run, spec(home.path(), "exec /bin/sleep 30"), &cx).unwrap();
    let guard = run.guard_identity().unwrap().clone();
    // SAFETY: this is the exact guard started and retained by this fixture's
    // run. The owner has not reaped it, so its PID cannot have been reused.
    assert_eq!(unsafe { libc::kill(guard.pid as i32, libc::SIGKILL) }, 0);
    eventually(|| {
        !matches!(
            effigy_kernel::ident::process_state(guard.pid),
            Observed::Verified(
                effigy_kernel::ident::ProcessState::Active
                    | effigy_kernel::ident::ProcessState::Stopped,
                _
            )
        )
    });
    assert!(Custody::spawn_foreground(
        &mut run,
        spec(home.path(), "printf forbidden > forbidden"),
        &cx
    )
    .is_err());
    assert!(!home.path().join("forbidden").exists());
    drop(run);
    eventually(|| {
        !matches!(
            effigy_kernel::ident::process_identity(child.identity().identity.pid),
            Observed::Verified(_, _)
        )
    });
    assert!(!root.path().join("kernel.db").exists());
}

#[test]
fn foreground_owner_kill_retains_gate_until_memory_guard_closes_group() {
    foreground_owner_kill(false);
}

#[test]
fn foreground_tree_owner_kill_retains_ancestor_gates_until_leaf_guard_closes_group() {
    foreground_owner_kill(true);
}

fn foreground_owner_kill(tree: bool) {
    let home = home();
    let cx = context();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg(if tree {
                "foreground-tree-owner"
            } else {
                "foreground-owner"
            })
            .arg(home.path())
            .env("HOME", home.path())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.path().join("foreground.json").exists());
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.path().join("foreground.json")).unwrap())
            .unwrap();
    let process: ProcessReceipt = serde_json::from_value(data["process"].clone()).unwrap();
    let guard: effigy_kernel::ident::ProcessIdentity =
        serde_json::from_value(data["guard"].clone()).unwrap();
    struct ResumeGuard(effigy_kernel::ident::ProcessIdentity);
    impl Drop for ResumeGuard {
        fn drop(&mut self) {
            // SAFETY: only the memory guard started by this fixture is named;
            // generation is rechecked before resuming on a failure path.
            if effigy_kernel::ident::process_identity(self.0.pid).verified() == Some(&self.0) {
                unsafe {
                    libc::kill(self.0.pid as i32, libc::SIGCONT);
                }
            }
        }
    }
    let resume = ResumeGuard(guard.clone());
    // SAFETY: exact guard of the newly started, live fixture; no PID scan.
    assert_eq!(unsafe { libc::kill(guard.pid as i32, libc::SIGSTOP) }, 0);
    eventually(|| {
        matches!(
            effigy_kernel::ident::process_state(guard.pid),
            Observed::Verified(effigy_kernel::ident::ProcessState::Stopped, _)
        )
    });
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    if tree {
        let ancestors: Vec<effigy_kernel::ident::ProcessIdentity> =
            serde_json::from_value(data["ancestor_guards"].clone()).unwrap();
        assert_eq!(ancestors.len(), 2);
        // Let both ancestor guards exit while leaf cleanup is paused. Their
        // descriptor copies alone must no longer be what holds the root.
        for ancestor in ancestors {
            eventually(|| {
                effigy_kernel::ident::process_identity(ancestor.pid).verified() != Some(&ancestor)
            });
        }
    }
    let observe = cx.operation().observe().unwrap();
    let root = StateRoot::observe(&observe, home.path())
        .verified()
        .unwrap()
        .path()
        .to_path_buf();
    assert!(!root.join("kernel.db").exists());
    let state = match StateRoot::observe(&observe, home.path()) {
        Observed::Verified(root, _) => root,
        other => panic!("{other:?}"),
    };
    assert!(matches!(
        effigy_kernel::gate::Gate::probe(&observe, &state, &invocation().spec().owner),
        Observed::Verified(effigy_kernel::gate::GateState::Held(_), _)
    ));
    assert_eq!(
        effigy_kernel::ident::process_identity(process.identity.pid).verified(),
        Some(&process.identity)
    );
    drop(resume);
    eventually(|| {
        matches!(
            effigy_kernel::gate::Gate::probe(&observe, &state, &invocation().spec().owner),
            Observed::Verified(effigy_kernel::gate::GateState::Free, _)
        )
    });
    eventually(|| {
        effigy_kernel::ident::process_identity(process.identity.pid).verified()
            != Some(&process.identity)
    });
    let output = Command::new("/bin/ps")
        .args(["-axo", "pgid="])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim() == process.pgid.to_string()));
}

/// A durable run that crashed and left unsettled residue is not a refusal for
/// the foreground form of the same task: the foreground run holds the owner
/// flock, which proves no predecessor can still be live (F11). The residue is
/// left for the durable path to settle and the journal is not written.
#[test]
fn foreground_runs_beside_unsettled_durable_residue_without_database_writes() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let previous = Run::begin(
        &cx,
        &root,
        &invocation(),
        BeginOpts {
            id: id(2),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: None,
        },
        &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
        &mut Strict,
    )
    .unwrap();
    drop(previous);
    let before = std::fs::read(root.path().join("kernel.db")).unwrap();
    let mut running = ForegroundRun::begin(
        &cx,
        &root,
        &invocation(),
        BeginOpts {
            id: id(1),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
    )
    .expect("a dead predecessor's residue never blocks the foreground form");
    running.interrupt(WorkloadOutcome::NotStarted).unwrap();
    running
        .settle(&cx.operation().cleanup(Duration::from_secs(5)).unwrap())
        .unwrap();
    assert_eq!(
        before,
        std::fs::read(root.path().join("kernel.db")).unwrap(),
        "a foreground run reads the residue and writes nothing"
    );
}

#[test]
fn foreground_same_owner_is_busy_and_never_reclaims_a_live_gate() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let run = foreground_begin(&cx, &root);
    let refused = ForegroundRun::begin(
        &cx,
        &root,
        &invocation(),
        BeginOpts {
            id: id(2),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
    );
    assert!(matches!(
        refused,
        Err(RunError::Gate(effigy_kernel::gate::GateError::Busy { .. }))
    ));
    drop(run);
    assert!(!root.path().join("kernel.db").exists());
}

#[test]
fn custody_completed_machine_store_run_closes_sentinel_and_releases_gate() {
    let home = home();
    let cx = context();
    let root = StateRoot::machine_effects(&cx, home.path()).unwrap();
    let mut coordinates = invocation().spec().clone();
    coordinates.task = TaskId::new(
        CheckoutId::new(CheckoutKind::Machine, [0; 16]),
        "",
        Surface::Published,
        "gateway-setup",
        None,
    )
    .unwrap();
    coordinates.owner = ResourceId::store(Scope::Machine, "gateway-setup").unwrap();
    let owner = coordinates.owner.clone();
    let mut run = Run::begin(
        &cx,
        &root,
        &ResolvedInvocation::new(coordinates).unwrap(),
        BeginOpts {
            id: id(1),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
        &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
        &mut Strict,
    )
    .unwrap();
    let sentinel = metadata(&cx.operation().observe().unwrap(), &root, id(1))
        .verified()
        .unwrap()
        .sentinel
        .clone()
        .unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let cleanup = cx.operation().cleanup(Duration::from_secs(2)).unwrap();
    assert!(run.settle(&cleanup, &mut Strict).unwrap().result().is_ok());
    assert_ne!(
        effigy_kernel::ident::process_identity(sentinel.pid).verified(),
        Some(&sentinel)
    );
    let reader = effigy_kernel::store::Store::read(&cx.operation().observe().unwrap(), &root);
    assert!(matches!(
        summary(reader.verified().unwrap(), id(1)),
        Observed::Absent(_)
    ));
    let _gate = effigy_kernel::gate::Gate::acquire(&cx, &root, &[owner], Wait::FailFast).unwrap();
}

#[test]
fn custody_machine_journal_routes_supervisor_requests_and_sentinel_recovery_without_legacy_writes()
{
    for killed in [false, true] {
        let home = home();
        let cx = context();
        let legacy = StateRoot::effects(&cx, home.path()).unwrap();
        drop(effigy_kernel::store::Store::open(&cx, &legacy).unwrap());
        let legacy_before = std::fs::read(legacy.path().join("kernel.db")).unwrap();
        let machine = StateRoot::machine_effects(&cx, home.path()).unwrap();
        let mut coordinates = invocation().spec().clone();
        coordinates.task = TaskId::new(
            CheckoutId::new(CheckoutKind::Machine, [0; 16]),
            "",
            Surface::Published,
            "gateway",
            None,
        )
        .unwrap();
        coordinates.owner = ResourceId::gateway("fixture-machine").unwrap();
        let invocation = ResolvedInvocation::new(coordinates).unwrap();
        let mut client = start_detached_at(
            &cx,
            &machine,
            &invocation,
            id(1),
            &next(),
            vec![spec(
                home.path(),
                "printf machine-output; printf machine-error >&2; exec /bin/sleep 30",
            )],
            &Executable::capture(&fixture()).unwrap(),
            ClientMode::Owning,
        )
        .unwrap();
        while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
        if let Ok(entries) = std::fs::read_dir(machine.path().join("runs").join(id(1).to_string()))
        {
            for entry in entries {
                assert!(
                    !entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".log"),
                    "machine output must remain in memory"
                );
            }
        }
        if killed {
            let metadata = metadata(&cx.operation().observe().unwrap(), &machine, id(1));
            let supervisor = metadata.verified().unwrap().supervisor.as_ref().unwrap();
            assert_eq!(
                effigy_kernel::ident::process_identity(supervisor.pid).verified(),
                Some(supervisor)
            );
            // SAFETY: the private fixture's exact newly admitted supervisor is
            // still owned by this client; its fresh boot/start identity matches.
            assert_eq!(
                unsafe { libc::kill(supervisor.pid as i32, libc::SIGKILL) },
                0
            );
        } else {
            client.stop(&cx, id(2)).unwrap();
        }
        eventually(|| {
            let observe = cx.operation().observe().unwrap();
            let Observed::Verified(reader, _) =
                effigy_kernel::store::Store::read(&observe, &machine)
            else {
                return false;
            };
            matches!(effigy_kernel::run::machine_completion(&reader, &invocation.spec().owner),
                Observed::Verified(completion, _) if completion.run == id(1))
        });
        let read = effigy_kernel::store::Store::read(&cx.operation().observe().unwrap(), &machine);
        assert!(
            matches!(
                effigy_kernel::run::summary(read.verified().unwrap(), id(1)),
                Observed::Absent(_)
            ),
            "completed machine runs retain no history"
        );
        let waited = client.wait(cx.operation());
        if killed {
            assert_eq!(
                waited.unwrap_err().kind(),
                std::io::ErrorKind::UnexpectedEof
            );
        } else {
            waited.unwrap();
        }
        assert_eq!(
            std::fs::read(legacy.path().join("kernel.db")).unwrap(),
            legacy_before
        );
    }
}

fn foreground_child(
    parent: &mut ForegroundRun,
    cx: &EffectContext,
    root: &StateRoot,
) -> ForegroundRun {
    let mut invocation = invocation().spec().clone();
    invocation.task = TaskId::new(
        invocation.task.checkout(),
        "",
        Surface::Published,
        "child",
        None,
    )
    .unwrap();
    invocation.owner = invocation.task.resource();
    parent
        .begin_child(
            root,
            &ResolvedInvocation::new(invocation).unwrap(),
            BeginOpts {
                id: id(2),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: Some(LifecyclePlan {
                    executable: Executable::capture(&fixture()).unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline,
                }),
            },
        )
        .unwrap_or_else(|error| panic!("child admission: {error}; context: {cx:?}"))
}

#[test]
fn foreground_parent_requires_positive_child_completion() {
    for complete in [false, true] {
        let home = home();
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        let mut parent = foreground_begin(&cx, &root);
        let mut child = foreground_child(&mut parent, &cx, &root);
        assert!(parent.finish(WorkloadOutcome::Succeeded).is_err());
        if complete {
            let mut process =
                Custody::spawn_foreground(&mut child, spec(home.path(), "exit 0"), &cx).unwrap();
            let exit = *process.wait(cx.operation()).verified().unwrap();
            process.complete_foreground(&mut child, exit).unwrap();
            child.finish(WorkloadOutcome::Succeeded).unwrap();
            child
                .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
                .unwrap();
            parent.finish(WorkloadOutcome::Succeeded).unwrap();
            parent
                .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
                .unwrap();
        } else {
            drop(child);
            assert!(parent.finish(WorkloadOutcome::Succeeded).is_err());
        }
        assert!(!root.path().join("kernel.db").exists());
    }
}

#[test]
fn foreground_auxiliary_custody_is_not_a_completion_obligation() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = foreground_begin(&cx, &root);
    let process =
        Custody::spawn_auxiliary_foreground(&mut run, spec(home.path(), "exec /bin/sleep 30"), &cx)
            .unwrap();
    let identity = process.identity().identity.clone();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    let outcome = run
        .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
        .unwrap();
    assert_eq!(outcome.exit_code(), 0);
    assert_ne!(
        effigy_kernel::ident::process_identity(identity.pid).verified(),
        Some(&identity)
    );
    assert!(!root.path().join("kernel.db").exists());
}

/// Q-011: custody hands a workload no parent capability, leaves slot 9 empty
/// and does not stop it opening a durable journal of its own. A nested run is
/// an ordinary invocation.
#[test]
fn a_foreground_child_receives_no_parent_capability_and_opens_its_own_journal() {
    let home = home();
    let cx = context();
    let root = StateRoot::foreground_effects(
        &cx,
        &effigy_kernel::location::CheckoutLocation {
            root: home.path().into(),
        },
    )
    .unwrap();
    let mut run = foreground_begin(&cx, &root);
    let journal_home = home.path().join("child-journal");
    std::fs::create_dir(&journal_home).unwrap();
    let mut spec = spec(home.path(), "unused");
    spec.argv = vec![fixture().into_os_string(), "capability-free".into()];
    spec.argv.push(journal_home.clone().into_os_string());
    let mut child = Custody::spawn_foreground(&mut run, spec, &cx).unwrap();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0, "the child ran as an ordinary invocation");
    child.complete_foreground(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    run.settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
        .unwrap();
    assert!(
        journal_home.join(".effigy/state/kernel.db").is_file(),
        "a nested workload journals on its own"
    );
    assert!(!root.path().join("kernel.db").exists());
}

#[test]
fn foreground_retained_git_pointer_refuses_new_workload_go() {
    let home = home();
    let cx = context();
    let one = home.path().join("git-one");
    let two = home.path().join("git-two");
    std::fs::create_dir(&one).unwrap();
    std::fs::create_dir(&two).unwrap();
    std::fs::write(home.path().join(".git"), "gitdir: git-one\n").unwrap();
    let location = effigy_kernel::location::CheckoutLocation {
        root: home.path().into(),
    };
    let root = StateRoot::foreground_effects(&cx, &location).unwrap();
    let mut run = foreground_begin(&cx, &root);
    // A Git pointer rewritten under a live run is ordinary: nothing retained is
    // compared, so the run continues and spawns (F15).
    std::fs::write(home.path().join(".git"), "gitdir: git-two\n").unwrap();
    let mut child =
        Custody::spawn_foreground(&mut run, spec(home.path(), "printf went"), &cx).unwrap();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0);
    child.complete_foreground(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    assert!(!root.path().join("kernel.db").exists());
    run.settle(&cx.operation().cleanup(Duration::from_secs(5)).unwrap())
        .unwrap();
}

/// A linked worktree's `.git` file is user data, not operator state: Git writes
/// it 0664 under umask 002, and a foreground task runs (F4).
#[test]
fn a_foreground_task_runs_in_a_linked_worktree_with_a_group_writable_git_file() {
    let home = home();
    let cx = context();
    let common = home.path().join("main-git/worktrees/linked");
    std::fs::create_dir_all(&common).unwrap();
    let checkout = home.path().join("linked");
    std::fs::create_dir_all(&checkout).unwrap();
    let pointer = checkout.join(".git");
    std::fs::write(&pointer, format!("gitdir: {}\n", common.display())).unwrap();
    std::fs::set_permissions(&pointer, std::fs::Permissions::from_mode(0o664)).unwrap();
    let location = effigy_kernel::location::CheckoutLocation {
        root: checkout.clone(),
    };
    let root = StateRoot::foreground_effects(&cx, &location)
        .expect("a worktree's `.git` pointer is user data, not operator state");
    let mut run = foreground_begin(&cx, &root);
    let mut child =
        Custody::spawn_foreground(&mut run, spec(&checkout, "printf linked"), &cx).unwrap();
    let exit = child.wait(cx.operation()).verified().copied().unwrap();
    assert_eq!(exit.code, 0);
    child.complete_foreground(&mut run, exit).unwrap();
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(
        std::fs::symlink_metadata(&pointer)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o664,
        "the worktree pointer is read and left exactly as it was"
    );
    run.settle(&cx.operation().cleanup(Duration::from_secs(5)).unwrap())
        .unwrap();
}

#[test]
fn foreground_git_replacement_keeps_directory_exclusion_until_old_custody_closes() {
    let home = home();
    let cx = context();
    let git = home.path().join(".git");
    std::fs::create_dir(&git).unwrap();
    let location = || effigy_kernel::location::CheckoutLocation {
        root: home.path().into(),
    };
    let observe = cx.operation().observe().unwrap();
    let old_id = StateRoot::filesystem_checkout(&observe, &location()).unwrap();
    let old_root = StateRoot::foreground_effects(&cx, &location()).unwrap();
    let opts = |n| BeginOpts {
        id: id(n),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: Some(LifecyclePlan {
            executable: Executable::capture(&fixture()).unwrap(),
            placement: effigy_kernel::supervise::Placement::Inline,
        }),
    };
    let mut old =
        ForegroundRun::begin(&cx, &old_root, &invocation_at_checkout(old_id), opts(18)).unwrap();
    std::fs::rename(&git, home.path().join("git-retired")).unwrap();
    std::fs::create_dir(&git).unwrap();
    // Identity is the checkout's location, not a directory generation
    // (effigy#337): replacing `.git` in place keeps the same gate, and the
    // old custody still holds it.
    let new_id = StateRoot::filesystem_checkout(&observe, &location()).unwrap();
    assert_eq!(old_id, new_id);
    let new_root = StateRoot::foreground_effects(&cx, &location()).unwrap();
    assert!(matches!(
        ForegroundRun::begin(&cx, &new_root, &invocation_at_checkout(new_id), opts(19)),
        Err(RunError::Gate(effigy_kernel::gate::GateError::Busy { .. }))
    ));
    old.interrupt(WorkloadOutcome::NotStarted).unwrap();
    old.settle(&cx.operation().cleanup(Duration::from_secs(5)).unwrap())
        .unwrap();
    let mut new =
        ForegroundRun::begin(&cx, &new_root, &invocation_at_checkout(new_id), opts(19)).unwrap();
    new.finish(WorkloadOutcome::Succeeded).unwrap();
    new.settle(&cx.operation().cleanup(Duration::from_secs(5)).unwrap())
        .unwrap();
    assert!(!new_root.path().join("kernel.db").exists());
}

#[test]
fn foreground_filesystem_identity_cannot_be_promoted_into_a_durable_run() {
    let home = home();
    let cx = context();
    let location = effigy_kernel::location::CheckoutLocation {
        root: home.path().into(),
    };
    let checkout =
        StateRoot::filesystem_checkout(&cx.operation().observe().unwrap(), &location).unwrap();
    let root = StateRoot::foreground_effects(&cx, &location).unwrap();
    let task = TaskId::new(checkout, "", Surface::Published, "fixture", None).unwrap();
    let original = invocation();
    let mut spec = original.spec().clone();
    spec.owner = task.resource();
    spec.task = task;
    let invocation = ResolvedInvocation::new(spec).unwrap();
    assert!(Run::begin(
        &cx,
        &root,
        &invocation,
        BeginOpts {
            id: id(3),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: None
        },
        &cx.operation().cleanup(Duration::from_secs(5)).unwrap(),
        &mut Strict
    )
    .is_err());
    assert!(!root.path().join("kernel.db").exists());
    assert!(!root.path().join("runs").exists());
}

fn invocation_at_checkout(checkout: CheckoutId) -> ResolvedInvocation {
    let mut spec = invocation().spec().clone();
    spec.task = TaskId::new(checkout, "", Surface::Published, "fixture", None).unwrap();
    spec.owner = spec.task.resource();
    ResolvedInvocation::new(spec).unwrap()
}

#[test]
fn foreground_and_durable_exclusion_survives_first_token_creation() {
    let home = home();
    let cx = context();
    let location = effigy_kernel::location::CheckoutLocation {
        root: home.path().into(),
    };
    let local =
        StateRoot::filesystem_checkout(&cx.operation().observe().unwrap(), &location).unwrap();
    let transient = StateRoot::foreground_effects(&cx, &location).unwrap();
    let foreground_invocation = invocation_at_checkout(local);
    let mut foreground = ForegroundRun::begin(
        &cx,
        &transient,
        &foreground_invocation,
        BeginOpts {
            id: id(11),
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::capture(&fixture()).unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
    )
    .unwrap();
    let machine = home.path().join("machine");
    std::fs::create_dir(&machine).unwrap();
    let durable_root = StateRoot::checkout_effects(&cx, &machine, &location).unwrap();
    let token = CheckoutId::new(CheckoutKind::Primary, [17; 16]);
    std::fs::write(
        home.path().join(".effigy/effigy-runtime-scope"),
        "11111111111111111111111111111111\n",
    )
    .unwrap();
    let durable_invocation = invocation_at_checkout(token);
    let opts = || BeginOpts {
        id: id(12),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: None,
    };
    let cleanup = cx.operation().cleanup(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        Run::begin(
            &cx,
            &durable_root,
            &durable_invocation,
            opts(),
            &cleanup,
            &mut Strict
        ),
        Err(RunError::Gate(_))
    ));
    assert!(!durable_root.path().join("kernel.db").exists());
    foreground.finish(WorkloadOutcome::Succeeded).unwrap();
    foreground.settle(&cleanup).unwrap();
    let mut durable = Run::begin(
        &cx,
        &durable_root,
        &durable_invocation,
        opts(),
        &cleanup,
        &mut Strict,
    )
    .unwrap();
    assert!(matches!(
        effigy_kernel::gate::Gate::probe(
            &cx.operation().observe().unwrap(),
            &transient,
            &foreground_invocation.spec().owner
        ),
        Observed::Verified(effigy_kernel::gate::GateState::Held(_), _)
    ));
    assert!(matches!(
        ForegroundRun::begin(
            &cx,
            &transient,
            &durable_invocation,
            BeginOpts {
                id: id(13),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: Some(LifecyclePlan {
                    executable: Executable::capture(&fixture()).unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline
                })
            }
        ),
        Err(RunError::Gate(_))
    ));
    durable.finish(WorkloadOutcome::Succeeded).unwrap();
    durable.settle(&cleanup, &mut Strict).unwrap();
}

#[test]
fn custody_sentinel_recovery_meets_the_filesystem_gate_alias() {
    use effigy_kernel::{
        gate::{Gate, GateState},
        ident,
    };
    let home = home();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg("inline-alias")
            .arg(home.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.path().join("ready").exists());
    let cx = context();
    let observe = cx.operation().observe().unwrap();
    let root = StateRoot::observe(&observe, home.path())
        .verified()
        .unwrap()
        .clone();
    let location = effigy_kernel::location::CheckoutLocation {
        root: home.path().into(),
    };
    let local = StateRoot::filesystem_checkout(&observe, &location).unwrap();
    let alias = invocation_at_checkout(local).spec().owner.clone();
    assert!(matches!(
        Gate::probe(&observe, &root, &alias),
        Observed::Verified(GateState::Held(_), _)
    ));
    let metadata = metadata(&observe, &root, RunId::new(1, [9; 10]).unwrap());
    let sentinel = metadata.verified().unwrap().sentinel.clone().unwrap();
    let process: ProcessReceipt =
        serde_json::from_slice(&std::fs::read(home.path().join("receipt")).unwrap()).unwrap();
    struct Resume(ident::ProcessIdentity);
    impl Drop for Resume {
        fn drop(&mut self) {
            if ident::process_identity(self.0.pid).verified() == Some(&self.0) {
                // SAFETY: exact identity of the sentinel this test started.
                unsafe {
                    libc::kill(self.0.pid as i32, libc::SIGCONT);
                }
            }
        }
    }
    assert_eq!(
        ident::process_identity(sentinel.pid).verified(),
        Some(&sentinel)
    );
    // SAFETY: this fixture owns this freshly verified sentinel.
    assert_eq!(unsafe { libc::kill(sentinel.pid as i32, libc::SIGSTOP) }, 0);
    let resume = Resume(sentinel.clone());
    eventually(|| {
        matches!(
            ident::process_state(sentinel.pid),
            Observed::Verified(ident::ProcessState::Stopped, _)
        )
    });
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    let effects_root = StateRoot::effects(&cx, home.path()).unwrap();
    let blocker = Gate::acquire(&cx, &effects_root, &[alias], Wait::FailFast).unwrap();
    drop(resume);
    // The multi-gate acquire writes diagnostics only after every lock is
    // held. While blocked on our alias, the owner gate can still name the
    // dead predecessor; observe the lock and the exact live sentinel itself.
    eventually(|| {
        matches!(
            Gate::probe(&observe, &root, &invocation().spec().owner),
            Observed::Verified(GateState::Held(_), _)
        )
    });
    assert_eq!(
        ident::process_identity(sentinel.pid).verified(),
        Some(&sentinel)
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        ident::process_identity(process.identity.pid).verified(),
        Some(&process.identity)
    );
    assert!(!matches!(
        fresh_status(&observe, &root, RunId::new(1, [9; 10]).unwrap()),
        Observed::Verified(RunStatus::Closed { .. }, _)
    ));
    drop(blocker);
    eventually(|| {
        matches!(
            fresh_status(&observe, &root, RunId::new(1, [9; 10]).unwrap()),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
    eventually(|| {
        ident::process_identity(process.identity.pid).verified() != Some(&process.identity)
    });
}

#[test]
fn custody_deleted_checkout_lease_survives_owner_kill_until_positive_group_closure() {
    let home = home();
    let mut owner = ChildGuard(
        Command::new(fixture())
            .arg("checkout-lease-owner")
            .arg(home.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    eventually(|| home.path().join("lease-ready.json").is_file());
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.path().join("lease-ready.json")).unwrap())
            .unwrap();
    let expected: effigy_kernel::gate::CheckoutFenceIdentity =
        serde_json::from_value(value["identity"].clone()).unwrap();
    let process: ProcessReceipt = serde_json::from_value(value["child"].clone()).unwrap();
    // Contract 052 keeps the anchor as positive evidence of the original group
    // identity: deleting the checkout does not prove group closure.
    let anchor: effigy_kernel::ident::ProcessIdentity =
        serde_json::from_value(value["anchor"].clone()).unwrap();
    let anchor_live = || {
        matches!(
            effigy_kernel::ident::process_state(anchor.pid),
            Observed::Verified(
                effigy_kernel::ident::ProcessState::Active
                    | effigy_kernel::ident::ProcessState::Stopped,
                _
            )
        )
    };
    let cx = context();
    let machine = StateRoot::runtime_machine_effects(&cx, &home.path().join("machine")).unwrap();
    let token = "04040404040404040404040404040404";
    assert!(effigy_kernel::gate::Gate::checkout_retirement(
        &cx,
        &machine,
        token,
        expected,
        Wait::FailFast
    )
    .is_err());
    std::fs::remove_dir_all(home.path().join("checkout")).unwrap();
    assert!(
        anchor_live(),
        "the anchor is the group evidence a deleted checkout still needs"
    );
    assert!(effigy_kernel::gate::Gate::checkout_retirement(
        &cx,
        &machine,
        token,
        expected,
        Wait::FailFast
    )
    .is_err());
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        effigy_kernel::gate::Gate::checkout_retirement(
            &cx,
            &machine,
            token,
            expected,
            Wait::FailFast
        )
        .is_err(),
        "cleanup retains the shared lease through the resistant process grace"
    );
    eventually(|| {
        effigy_kernel::gate::Gate::checkout_retirement(
            &cx,
            &machine,
            token,
            expected,
            Wait::FailFast,
        )
        .is_ok()
    });
    assert!(
        matches!(
            effigy_kernel::ident::process_identity(process.identity.pid),
            Observed::Absent(_)
        ) || matches!(
            effigy_kernel::ident::process_state(process.identity.pid),
            Observed::Verified(effigy_kernel::ident::ProcessState::Zombie, _)
        )
    );
    assert!(
        !anchor_live(),
        "settlement stops the anchor once the workload group is closed"
    );
    assert!(!machine.path().join("machine.db").exists());
    assert!(!machine.path().join("kernel.db").exists());
}

#[test]
fn custody_workspace_orphan_refuses_handles_until_journal_recovery() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    workspace_session(&cx, &root);
    let key = AdmissionKey {
        session: Session {
            run: id(7),
            generation: Generation::new(1),
        },
        request: RequestId::new([71; 16]),
    };
    let digest = Digest::of(b"workspace-orphan");
    let ticket = match reserve(&cx, &root, key, digest.clone(), id(9)).unwrap() {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("reservation {other:?}"),
    };
    let mut client = start_engine(
        &cx,
        home.path(),
        EngineLaunch {
            payload: home.path().as_os_str().as_encoded_bytes().to_vec(),
            cwd: home.path().into(),
            env: vec![],
            handoff: None,
            admission: Some(ticket),
        },
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    )
    .unwrap();
    while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
    let observe = cx.operation().observe().unwrap();
    let info = metadata(&observe, &root, id(9));
    let info = info.verified().unwrap();
    let supervisor = info.supervisor.as_ref().unwrap();
    assert_eq!(
        effigy_kernel::ident::process_identity(supervisor.pid).verified(),
        Some(supervisor)
    );
    // SAFETY: stop only this fixture's identity-checked supervisor so it
    // cannot settle between the two deaths we are deliberately exercising.
    assert_eq!(
        unsafe { libc::kill(supervisor.pid as i32, libc::SIGSTOP) },
        0
    );
    eventually(|| {
        matches!(
            effigy_kernel::ident::process_state(supervisor.pid),
            Observed::Verified(effigy_kernel::ident::ProcessState::Stopped, _)
        )
    });
    // Both identities belong to this test's start, freshly checked before kill.
    for identity in [
        info.sentinel.as_ref().unwrap(),
        info.supervisor.as_ref().unwrap(),
    ] {
        assert_eq!(
            effigy_kernel::ident::process_identity(identity.pid).verified(),
            Some(identity)
        );
        // SAFETY: kill only an identity-checked fixture process started above.
        assert_eq!(unsafe { libc::kill(identity.pid as i32, libc::SIGKILL) }, 0);
    }
    assert!(client.wait(cx.operation()).is_err());
    assert!(matches!(
        lookup(&observe, &root, key, digest.clone()),
        Observed::Verified(Recovered::Unknown { .. }, _)
    ));
    assert!(resume_owner(&cx, &root, key.session, id(9)).is_err());
    assert!(attach_secondary(&cx, &root, key.session, id(9), AttachmentId::new([71; 16])).is_err());
    assert!(matches!(
        reserve(&cx, &root, key, digest.clone(), id(10)).unwrap(),
        Reservation::Recorded(Recovered::Unknown { .. })
    ));
    let outcome = settle_interrupted(
        &cx,
        &root,
        &invocation().spec().owner,
        id(9),
        next(),
        &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
        &mut Strict,
    )
    .unwrap();
    assert_eq!(outcome.cleanup(), CleanupOutcome::Complete);
    assert!(workload_gone(home.path()));
    assert!(
        matches!(lookup(&observe, &root, key, digest), Observed::Verified(Recovered::Completed { run, .. }, _) if run == id(9))
    );
}

/// Both an escalation queued before keyed admission and a new escalation sent
/// to its current successor remain serviceable during predecessor recovery.
#[test]
fn custody_keyed_successor_services_escalation_during_predecessor_recovery() {
    for queued_before in [true, false] {
        let home = home();
        eprintln!(
            "keyed recovery queued_before={queued_before}: {}",
            home.path().display()
        );
        let cx = context();
        let root = StateRoot::effects(&cx, home.path()).unwrap();
        workspace_session(&cx, &root);
        let mut client = start_engine(
            &cx,
            home.path(),
            EngineLaunch {
                payload: home.path().as_os_str().as_encoded_bytes().to_vec(),
                cwd: home.path().into(),
                env: vec![("FIXTURE_ENGINE_LONG_GRACE".into(), "1".into())],
                handoff: None,
                admission: None,
            },
            &Executable::capture(&fixture()).unwrap(),
            ClientMode::Owning,
        )
        .unwrap();
        while !matches!(client.event(cx.operation()).unwrap(), Event::Active) {}
        eventually(|| home.path().join("engine-workload.pid").exists());
        let observe = cx.operation().observe().unwrap();
        let info = metadata(&observe, &root, id(9));
        let info = info.verified().unwrap();
        let supervisor = info.supervisor.as_ref().unwrap();
        assert_eq!(
            effigy_kernel::ident::process_identity(supervisor.pid).verified(),
            Some(supervisor)
        );
        // SAFETY: stop only this test's identity-checked supervisor.
        assert_eq!(
            unsafe { libc::kill(supervisor.pid as i32, libc::SIGSTOP) },
            0
        );
        eventually(|| {
            matches!(
                effigy_kernel::ident::process_state(supervisor.pid),
                Observed::Verified(effigy_kernel::ident::ProcessState::Stopped, _)
            )
        });
        let old_fence = client.fence.clone();
        if queued_before {
            client.escalate(&cx, id(81)).unwrap();
        }
        for identity in [info.sentinel.as_ref().unwrap(), supervisor] {
            assert_eq!(
                effigy_kernel::ident::process_identity(identity.pid).verified(),
                Some(identity)
            );
            // SAFETY: kill only this fixture's recorded, verified processes.
            assert_eq!(unsafe { libc::kill(identity.pid as i32, libc::SIGKILL) }, 0);
        }
        assert!(client.wait(cx.operation()).is_err());
        assert!(
            !workload_gone(home.path()),
            "predecessor must still need custody recovery"
        );
        let key = AdmissionKey {
            session: Session {
                run: id(7),
                generation: Generation::new(1),
            },
            request: RequestId::new([81; 16]),
        };
        let digest = Digest::of(b"keyed recovery escalation");
        let ticket = match reserve(&cx, &root, key, digest.clone(), id(10)).unwrap() {
            Reservation::Fresh(ticket) => ticket,
            other => panic!("ticket {other:?}"),
        };
        let sender = if queued_before {
            None
        } else {
            let root = root.clone();
            Some(std::thread::spawn(move || {
                let cx = context();
                let observe = cx.operation().observe().unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    if let Observed::Verified(Recovered::Admitted { run, fence }, _) =
                        lookup(&observe, &root, key, digest.clone())
                    {
                        assert_eq!(run, id(10));
                        assert!(
                            request(
                                &cx,
                                &root,
                                &Request {
                                    id: id(83),
                                    run: id(9),
                                    expected: old_fence.clone(),
                                    action: RequestAction::Escalate
                                }
                            )
                            .is_err(),
                            "stale public controls stay refused"
                        );
                        request(
                            &cx,
                            &root,
                            &Request {
                                id: id(82),
                                run,
                                expected: fence,
                                action: RequestAction::Escalate,
                            },
                        )
                        .unwrap();
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "successor never became recoverable"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
            }))
        };
        let cx = context();
        let started = Instant::now();
        let mut successor = Run::begin_admitted(
            &cx,
            &root,
            &invocation(),
            BeginOpts {
                id: id(10),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: None,
            },
            &cx.operation().cleanup(Duration::from_secs(15)).unwrap(),
            &mut Strict,
            ticket,
        )
        .unwrap();
        if let Some(sender) = sender {
            sender.join().unwrap();
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "recovery ignored escalation and waited for the ten-second grace: {:?}",
            started.elapsed()
        );
        assert!(workload_gone(home.path()));
        if queued_before {
            successor.finish(WorkloadOutcome::Succeeded).unwrap();
        } else {
            assert!(successor
                .requests()
                .unwrap()
                .iter()
                .any(|r| matches!(r.action, RequestAction::Escalate)));
            successor
                .interrupt(
                    &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
                    WorkloadOutcome::Cancelled {
                        signal: None,
                        cause: StopCause::Requested,
                    },
                )
                .unwrap();
        }
        successor
            .settle(
                &cx.operation().cleanup(Duration::from_secs(10)).unwrap(),
                &mut Strict,
            )
            .unwrap();
    }
}
