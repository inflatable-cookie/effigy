//! Real macOS/Linux custody qualification. All processes and files are fixtures.
use effigy_kernel::{
    ctx::*, custody::*, gate::Wait, id::*, observe::*, outcome::*, root::StateRoot, run::*,
    supervise::*,
};
use std::collections::BTreeMap;
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
    tempfile::Builder::new()
        .prefix("kernel-custody-")
        .tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap())
        .unwrap()
}
fn fixture() -> PathBuf {
    std::env::var_os("EFFIGY_KERNEL_CUSTODY_FIXTURE")
        .map(PathBuf::from)
        .expect("required custody fixture binary")
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
fn eventually(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(12);
    while !predicate() {
        assert!(Instant::now() < end, "fixture deadline");
        std::thread::sleep(Duration::from_millis(10));
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
        spec(home.path(), "(trap '' TERM; /bin/sleep 30) & exit 0"),
        &cx,
    )
    .unwrap();
    let receipt = child.identity();
    assert_eq!(
        child.wait(cx.operation()).verified(),
        Some(&Exit { code: 0 })
    );
    // Leader exit has not reaped or surrendered the group number.
    assert!(effigy_kernel::ident::process_identity(receipt.identity.pid)
        .verified()
        .is_some());
    let report = child.stop(&cx.operation().cleanup(Duration::from_secs(5)).unwrap());
    assert!(report.residue.is_empty(), "{report:?}");
    assert_eq!(report.capture, CaptureState::Complete);
    assert!(report
        .signals
        .iter()
        .all(|s| s.pid == receipt.pgid && s.group));
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
            let reader = match effigy_kernel::store::Store::read(
                &cx.operation().observe().unwrap(),
                &root,
            ) {
                Observed::Verified(r, _) => r,
                other => panic!("{other:?}"),
            };
            let runid = RunId::new(1, [9; 10]).unwrap();
            eventually(|| {
                matches!(
                    status(
                        &cx.operation().observe().unwrap(),
                        &root,
                        &reader,
                        runid,
                        next()
                    ),
                    Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
                )
            });
            let status = status(
                &cx.operation().observe().unwrap(),
                &root,
                &reader,
                runid,
                next(),
            );
            if let Observed::Verified(RunStatus::Closed { effects, .. }, _) = status {
                for effect in effects {
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
    let reader = effigy_kernel::store::Store::read(&cx.operation().observe().unwrap(), &root);
    let reader = reader.verified().unwrap();
    assert!(matches!(
        status(
            &cx.operation().observe().unwrap(),
            &root,
            reader,
            id(1),
            next()
        ),
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
            status(
                &cx.operation().observe().unwrap(),
                &root,
                reader,
                id(1),
                next()
            ),
            Observed::Verified(RunStatus::Closed { complete: true, .. }, _)
        )
    });
}
#[test]
fn custody_replaced_binary_refuses_startup_and_status_names_dead_sentinel() {
    let home = home();
    let copy = home.path().join("fixture-copy");
    std::fs::copy(fixture(), &copy).unwrap();
    let executable = Executable::capture(&copy).unwrap();
    std::fs::write(&copy, b"changed").unwrap();
    assert!(executable.verify().is_err());
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
