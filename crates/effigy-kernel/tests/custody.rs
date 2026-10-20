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
        .tempdir_in(std::fs::canonicalize("/tmp").unwrap())
        .unwrap()
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
            let reader = match effigy_kernel::store::Store::read(
                &cx.operation().observe().unwrap(),
                &root,
            ) {
                Observed::Verified(r, _) => r,
                other => panic!("{other:?}"),
            };
            let runid = RunId::new(1, [9; 10]).unwrap();
            eventually(|| {
                match status(
                    &cx.operation().observe().unwrap(),
                    &root,
                    &reader,
                    runid,
                    next(),
                ) {
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
                }
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
    let reader = effigy_kernel::store::Store::read(&observed, &root);
    let reader = reader.verified().unwrap();
    eventually(|| {
        match status(
            &observed,
            &root,
            reader,
            RunId::new(1, [9; 10]).unwrap(),
            next(),
        ) {
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
        }
    });
    assert_ne!(
        effigy_kernel::ident::process_identity(descendant).verified(),
        Some(&identity)
    );
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
    let reader = effigy_kernel::store::Store::read(&observe, &root1);
    std::thread::sleep(Duration::from_millis(100));
    assert!(matches!(
        status(&observe, &root1, reader.verified().unwrap(), runid, next()),
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
    let reader = effigy_kernel::store::Store::read(&observe, &root2);
    eventually(|| {
        matches!(
            status(&observe, &root2, reader.verified().unwrap(), id(1), next()),
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
    let reader = effigy_kernel::store::Store::read(&observe, &root);
    eventually(|| {
        matches!(
            status(&observe, &root, reader.verified().unwrap(), id(1), next()),
            Observed::Verified(RunStatus::Live(Phase::Stopping), _)
        )
    });
    let at = Instant::now();
    client.escalate(&cx, id(3)).unwrap();
    eventually(|| {
        matches!(
            status(&observe, &root, reader.verified().unwrap(), id(1), next()),
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
    let reader = effigy_kernel::store::Store::read(&observe, &root);
    eventually(|| {
        matches!(
            status(
                &observe,
                &root,
                reader.verified().unwrap(),
                RunId::new(1, [9; 10]).unwrap(),
                next()
            ),
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
    let reader = effigy_kernel::store::Store::read(&observe, &root);
    assert!(matches!(
        status(&observe, &root, reader.verified().unwrap(), runid, next()),
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
    let reader = effigy_kernel::store::Store::read(&observe, &root);
    status(
        &observe,
        &root,
        reader.verified().unwrap(),
        RunId::new(1, [9; 10]).unwrap(),
        next(),
    )
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
    let mut client = start_engine(
        &cx,
        home.path(),
        EngineLaunch {
            payload: home.path().as_os_str().as_encoded_bytes().to_vec(),
            cwd: home.path().into(),
            env: vec![],
        },
        &Executable::capture(&fixture()).unwrap(),
        ClientMode::Owning,
    )
    .unwrap();
    assert_eq!(client.run, RunId::new(1, [9; 10]).unwrap());
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
