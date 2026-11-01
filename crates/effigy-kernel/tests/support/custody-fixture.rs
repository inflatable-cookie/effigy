//! Disposable qualification binary only. Product dispatch/adoption is separate.
use effigy_kernel::{
    ctx::*, custody::*, gate::Wait, id::*, observe::*, outcome::*, root::StateRoot, run::*,
    signal::*, supervise::*,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};
#[derive(Debug)]
struct Strict;
impl SettlementAdapter for Strict {
    fn observe(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Fence> {
        panic!("unmodelled adapter observation")
    }
    fn release(&mut self, _: &Effect, _: &CleanupContext) -> Observed<()> {
        panic!("unmodelled adapter release")
    }
    fn reconcile(&mut self, _: &Effect, _: &CleanupContext) -> Observed<Reconciled> {
        panic!("unmodelled adapter reconcile")
    }
}
fn op(cancel: CancelToken) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
        cancel,
        Limits {
            capture_tail: 4096,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 4096,
            queue_bytes: 4096,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Host,
    )
}
fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap()
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
fn spec(home: &Path) -> ProcessSpec {
    ProcessSpec {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            "trap '' TERM; /bin/sleep 40 & wait".into(),
        ],
        cwd: home.into(),
        env: BTreeMap::new(),
        grace: Duration::from_millis(40),
        lifetime: Lifetime::Finite(Duration::from_secs(40)),
        terminal: Terminal::None,
        label: "fixture".into(),
    }
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let entry = args.first().and_then(|s| s.to_str()).unwrap_or("");
    let policy = match entry {
        "__kernel-sentinel" => RouterPolicy::Sentinel,
        "__kernel-supervise" => RouterPolicy::Supervisor,
        _ => RouterPolicy::Inline,
    };
    let mut router = SignalRouter::install(policy).unwrap();
    if let Some(result) = effigy_kernel::probe::internal_entry(entry, router.root_cancel()) {
        result.unwrap();
        return;
    }
    if entry.starts_with("probe-") {
        probe_fixture::run(entry, &args, router.root_cancel());
        return;
    }
    if entry == "__kernel-anchor" {
        // The anchor's image is the run's own executable, and an image that
        // carries a file capability or a setuid bit is a secure exec: the
        // kernel clears `dumpable`, so `/proc/<pid>/environ` refuses the
        // operator's own reads. The installed `effigy` carries
        // `cap_net_bind_service` for the low-port gateway, so a Linux gateway
        // setup anchor is exactly such an image (effigy#476). Model that
        // post-exec state here; the anchor's own entry must prove it can still
        // be read as the group's evidence.
        #[cfg(target_os = "linux")]
        unsafe {
            libc::prctl(libc::PR_SET_DUMPABLE, 0);
        }
    }
    if entry.starts_with("__kernel-") {
        let result = engine_entry(
            entry,
            router,
            &mut Strict,
            |a| a == [OsString::from("recover")] || a == ["effigy", "release", "run"],
            fixture_engine,
        );
        if !matches!(result, Some(Ok(()))) {
            std::process::exit(1);
        }
        return;
    }
    if entry == "capability-free" {
        // Nothing a run spawns carries a parent capability and nothing reads
        // one: a child that opens a durable journal is an ordinary run.
        assert!(std::env::var_os("EFFIGY_PARENT_CAPABILITY").is_none());
        // SAFETY: F_GETFD only queries this process's descriptor table.
        assert!(
            unsafe { libc::fcntl(9, libc::F_GETFD) } < 0,
            "slot 9 carries nothing"
        );
        let cx = op(router.root_cancel()).effects().unwrap();
        let home = Path::new(args.get(1).expect("private fixture home"));
        let root = StateRoot::effects(&cx, home).unwrap();
        effigy_kernel::store::Store::open(&cx, &root).unwrap();
        return;
    }
    if matches!(entry, "foreground-owner" | "foreground-tree-owner") {
        let home = Path::new(args.get(1).expect("private fixture root"));
        let cx = op(router.root_cancel()).effects().unwrap();
        let root = StateRoot::effects(&cx, home).unwrap();
        let mut run = ForegroundRun::begin(
            &cx,
            &root,
            &invocation(),
            BeginOpts {
                id: RunId::new(1, [1; 10]).unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: Some(LifecyclePlan {
                    executable: Executable::current().unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline,
                }),
            },
        )
        .unwrap();
        let mut ancestors = Vec::new();
        if entry == "foreground-tree-owner" {
            for n in 2..=3 {
                let mut child_invocation = invocation().spec().clone();
                child_invocation.task = TaskId::new(
                    child_invocation.task.checkout(),
                    "",
                    Surface::Published,
                    &format!("child-{n}"),
                    None,
                )
                .unwrap();
                child_invocation.owner = child_invocation.task.resource();
                let child_run = run
                    .begin_child(
                        &root,
                        &ResolvedInvocation::new(child_invocation).unwrap(),
                        BeginOpts {
                            id: RunId::new(1, [n; 10]).unwrap(),
                            wait: Wait::FailFast,
                            acknowledge: vec![],
                            next: next(),
                            custody: Some(LifecyclePlan {
                                executable: Executable::current().unwrap(),
                                placement: effigy_kernel::supervise::Placement::Inline,
                            }),
                        },
                    )
                    .unwrap();
                ancestors.push(run);
                run = child_run;
            }
        }
        let mut child = Custody::spawn_foreground(&mut run, spec(home), &cx).unwrap();
        std::fs::write(
            home.join("foreground.pending"),
            serde_json::to_vec(&serde_json::json!({
                "process": child.identity(), "guard": run.guard_identity(),
                "ancestor_guards": ancestors.iter().map(|run| run.guard_identity()).collect::<Vec<_>>(),
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::rename(
            home.join("foreground.pending"),
            home.join("foreground.json"),
        )
        .unwrap();
        loop {
            if let Some(signal) = router.poll().first() {
                let received = signal.received;
                run.interrupt(WorkloadOutcome::Cancelled {
                    cause: StopCause::Signal(received),
                    signal: Some(received),
                })
                .unwrap();
                run.settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
                    .unwrap();
                std::process::exit(128 + received);
            }
            let _ = child.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    if entry == "handoff-reader" {
        // Read the one-shot handoff slot directly. A shell cannot name a
        // two-digit descriptor: dash rejects `cat <&11` ("Bad fd number", exit 2)
        // and even `exec 11<&0`, so the fixture is the reader (effigy#520).
        use std::io::Read as _;
        use std::os::fd::FromRawFd as _;
        let out = Path::new(args.get(1).expect("handoff output path"));
        // SAFETY: the custody spawn placed the caller's one-shot reader at slot
        // 11 and cleared the slot's CLOEXEC flag before exec.
        let mut reader = unsafe { std::fs::File::from_raw_fd(HANDOFF_FD) };
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).expect("read the handoff");
        std::fs::write(out, bytes).expect("write the handoff payload");
        return;
    }
    if entry == "anchor-no-children" {
        let mut status = 0;
        // SAFETY: WNOHANG observes only this disposable workload's children;
        // the proof expects none and owns any unexpected child it could reap.
        let result = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
        if result != -1 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ECHILD) {
            std::process::exit(29);
        }
        return;
    }
    if entry == "checkout-lease-owner" {
        let home = Path::new(args.get(1).expect("fixture home"));
        let checkout = home.join("checkout");
        let machine = home.join("machine");
        std::fs::create_dir_all(&checkout).unwrap();
        std::fs::create_dir_all(&machine).unwrap();
        let op = op(router.root_cancel());
        let cx = op.effects().unwrap();
        let cleanup = op.cleanup(Duration::from_secs(10)).unwrap();
        let root = StateRoot::checkout_effects(
            &cx,
            &machine,
            &effigy_kernel::location::CheckoutLocation {
                root: checkout.clone(),
            },
        )
        .unwrap();
        let mut spec_inv = invocation().spec().clone();
        spec_inv.owner =
            ResourceId::environment(spec_inv.task.checkout(), "lease-fixture").unwrap();
        let invocation = ResolvedInvocation::new(spec_inv).unwrap();
        let mut run = Run::begin(
            &cx,
            &root,
            &invocation,
            BeginOpts {
                id: RunId::new(1, [9; 10]).unwrap(),
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: Some(LifecyclePlan {
                    executable: Executable::current().unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline,
                }),
            },
            &cleanup,
            &mut Strict,
        )
        .unwrap();
        let mut process_spec = spec(&checkout);
        process_spec.grace = Duration::from_secs(2);
        let mut child = Custody::spawn(&mut run, process_spec, &cx).unwrap();
        run.advance(Phase::Active).unwrap();
        std::fs::write(home.join("lease-ready.json"),serde_json::to_vec(&serde_json::json!({"identity":run.checkout_fence_identity(),"child":child.identity(),"anchor":child.anchor()})).unwrap()).unwrap();
        let _ = child.wait(&op);
        run.settle(&cleanup, &mut Strict).unwrap();
        return;
    }
    if entry == "anchor-owner-loss" {
        // effigy#520: the owner spawns a genuine custody child itself, so it holds
        // the receipt (identity, group, nonce) and the anchor identity, and the run
        // also has its own registered sentinel. It then stays alive and settles
        // nothing: the proof kills only this recorded owner, by pid.
        let home = Path::new(args.get(1).expect("fixture home"));
        let op = op(router.root_cancel());
        let cx = op.effects().unwrap();
        let cleanup = op.cleanup(Duration::from_secs(10)).unwrap();
        let root = StateRoot::effects(&cx, home).unwrap();
        let id = RunId::new(1, [9; 10]).unwrap();
        let inv = invocation();
        let mut run = Run::begin(
            &cx,
            &root,
            &inv,
            BeginOpts {
                id,
                wait: Wait::FailFast,
                acknowledge: vec![],
                next: next(),
                custody: Some(LifecyclePlan {
                    executable: Executable::current().unwrap(),
                    placement: effigy_kernel::supervise::Placement::Inline,
                }),
            },
            &cleanup,
            &mut Strict,
        )
        .unwrap();
        let mut workload = spec(home);
        workload.label = "owner loss".into();
        let child = Custody::spawn(&mut run, workload, &cx).unwrap();
        run.advance(Phase::Active).unwrap();
        let observe = cx.operation().observe().unwrap();
        let meta = metadata(&observe, &root, id)
            .verified()
            .cloned()
            .expect("run metadata");
        std::fs::write(
            home.join("owner-loss.pending"),
            serde_json::to_vec(&serde_json::json!({
                "run": id,
                "owner": std::process::id(),
                "process": child.identity(),
                "anchor": child.anchor(),
                "sentinel": meta.sentinel,
                "supervisor": meta.supervisor,
                "sentinel_status": format!("{:?}", meta.sentinel_status),
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::rename(
            home.join("owner-loss.pending"),
            home.join("owner-loss.json"),
        )
        .unwrap();
        std::fs::write(home.join("ready"), b"ready").unwrap();
        loop {
            // The owner never settles. Its sentinel recovers the run once this
            // process is killed by the recorded pid.
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if entry == "nonce-workload" {
        std::thread::sleep(Duration::from_secs(40));
        return;
    }
    if entry == "nonce-parent" {
        let home = Path::new(args.get(1).expect("fixture home"));
        #[expect(
            clippy::zombie_processes,
            reason = "The proof deliberately exits the leader before its descendant; the sentinel must recover it."
        )]
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("nonce-workload")
            .spawn()
            .unwrap();
        std::fs::write(home.join("descendant-pid"), child.id().to_string()).unwrap();
        return;
    }
    let home = Path::new(args.get(1).expect("fixture home"));
    let op = op(router.root_cancel());
    let cx = op.effects().unwrap();
    let cleanup = op.cleanup(Duration::from_secs(10)).unwrap();
    let root = if entry == "inline-alias" {
        let machine = home.join("machine");
        std::fs::create_dir(&machine).unwrap();
        StateRoot::checkout_effects(
            &cx,
            &machine,
            &effigy_kernel::location::CheckoutLocation { root: home.into() },
        )
        .unwrap()
    } else {
        StateRoot::effects(&cx, home).unwrap()
    };
    let id = RunId::new(1, [9; 10]).unwrap();
    let inv = invocation();
    if entry == "engine-client" {
        // An owning (or, with `detach`, detaching) client of one engine run.
        let detach = args.get(2).is_some_and(|mode| mode == "detach");
        let mut client = start_engine(
            &cx,
            home,
            engine_launch(home),
            &Executable::current().unwrap(),
            if detach {
                ClientMode::Detach
            } else {
                ClientMode::Owning
            },
        )
        .unwrap();
        if !detach {
            while !matches!(client.event(&op).unwrap(), Event::Active) {}
        }
        std::fs::write(home.join("client-ready"), client.run.to_string()).unwrap();
        if detach {
            return;
        }
        loop {
            let _ = client.event(&op.child(Duration::from_millis(50)));
        }
    }
    if entry == "detached" {
        let mut client = start_detached(
            &cx,
            home,
            &inv,
            id,
            &next(),
            vec![spec(home)],
            &Executable::current().unwrap(),
            ClientMode::Owning,
        )
        .unwrap();
        while !matches!(client.event(&op).unwrap(), Event::Active) {}
        std::fs::write(home.join("ready"), b"ready").unwrap();
        loop {
            for event in router.poll() {
                if event.forward == libc::SIGKILL {
                    client
                        .escalate(&cx, RunId::new(3, [8; 10]).unwrap())
                        .unwrap();
                } else {
                    client.stop(&cx, RunId::new(2, [8; 10]).unwrap()).unwrap();
                }
            }
            let event_context = if op.cancel_token().cause().is_some() {
                op.cleanup(Duration::from_secs(10))
                    .unwrap()
                    .operation()
                    .clone()
            } else {
                op.child(Duration::from_millis(20))
            };
            if matches!(client.event(&event_context), Ok(Event::Closed { .. })) {
                break;
            }
        }
        return;
    }
    let mut run = Run::begin(
        &cx,
        &root,
        &inv,
        BeginOpts {
            id,
            wait: Wait::FailFast,
            acknowledge: vec![],
            next: next(),
            custody: Some(LifecyclePlan {
                executable: Executable::current().unwrap(),
                placement: effigy_kernel::supervise::Placement::Inline,
            }),
        },
        &cleanup,
        &mut Strict,
    )
    .unwrap();
    if entry == "inline-terminal-sequence" {
        sequence_foreground(&mut run, &cx, &op, home);
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        let settled = run.settle(&cleanup, &mut Strict).unwrap();
        assert_eq!(settled.cleanup(), CleanupOutcome::Complete);
        assert_eq!(settled.workload(), &WorkloadOutcome::Succeeded);
        assert_owner_terminal(home, "sequence-settled");
        std::fs::write(home.join("sequence-done"), b"yes").unwrap();
        return;
    }
    let workload = if entry == "inline-terminal" {
        ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "test -t 0 && read word && test \"$word\" = terminal-input && printf terminal-ok"
                    .into(),
            ],
            terminal: Terminal::Foreground,
            ..spec(home)
        }
    } else if entry == "inline-orphan" {
        ProcessSpec {
            argv: vec![
                std::env::current_exe().unwrap().into_os_string(),
                "nonce-parent".into(),
                home.as_os_str().into(),
            ],
            ..spec(home)
        }
    } else if entry == "anchor-gone-orphan" {
        ProcessSpec {
            argv: vec!["/bin/sh".into(), "-c".into(), "exit 0".into()],
            ..spec(home)
        }
    } else {
        spec(home)
    };
    let mut child = Custody::spawn(&mut run, workload, &cx).unwrap();
    if matches!(entry, "inline-orphan" | "anchor-gone-orphan") {
        child.wait(&op);
    }
    if entry == "anchor-gone-orphan" {
        std::fs::write(
            home.join("anchor.json"),
            serde_json::to_vec(&serde_json::json!({
                "process": child.identity(),
                "anchor": child.anchor(),
            }))
            .unwrap(),
        )
        .unwrap();
    }

    std::fs::write(
        home.join("receipt"),
        serde_json::to_vec(&child.identity()).unwrap(),
    )
    .unwrap();
    std::fs::write(home.join("ready"), b"ready").unwrap();
    loop {
        if matches!(entry, "inline-orphan" | "anchor-gone-orphan") {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        for signal in router.poll() {
            child.forward(signal.forward);
        }
        if let Some(cause) = op.cancel_token().cause() {
            run.interrupt(
                &cleanup,
                WorkloadOutcome::Cancelled {
                    cause: cause.clone(),
                    signal: match cause {
                        StopCause::Signal(s) => Some(*s),
                        _ => None,
                    },
                },
            )
            .unwrap();
            break;
        }
        if let Observed::Verified(exit, _) = child.poll() {
            child.complete(&mut run, exit).unwrap();
            run.finish(if exit.code == 0 {
                WorkloadOutcome::Succeeded
            } else {
                WorkloadOutcome::Failed { code: exit.code }
            })
            .unwrap();
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = run.settle(&cleanup, &mut Strict).unwrap();
    assert_eq!(result.cleanup(), CleanupOutcome::Complete);
    if entry == "inline-terminal" {
        assert_eq!(result.workload(), &WorkloadOutcome::Succeeded);
        // SAFETY: observe our controlling terminal after the custody lease restored it.
        assert_eq!(unsafe { libc::tcgetpgrp(0) }, unsafe { libc::getpgrp() });
        std::fs::write(home.join("terminal-restored"), b"yes").unwrap();
    }
}

/// The owner's own group is the terminal's foreground group again.
fn assert_owner_terminal(home: &Path, marker: &str) {
    // SAFETY: observe our controlling terminal and process group only.
    let (foreground, owner) = unsafe { (libc::tcgetpgrp(0), libc::getpgrp()) };
    assert_eq!(
        foreground, owner,
        "{marker}: the owner gave up the terminal"
    );
    std::fs::write(home.join(marker), b"yes").unwrap();
}

/// Two `Terminal::Foreground` children in one run: the first writes to the
/// terminal, the second reads a line from it. Each child exits and is reaped
/// before the next spawns, so the owner is in the foreground throughout.
fn sequence_foreground(run: &mut Run<'_>, cx: &EffectContext, op: &OperationContext, home: &Path) {
    let write = ProcessSpec {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            "test -t 0 && printf 'first-ok' > /dev/tty && printf first-captured".into(),
        ],
        terminal: Terminal::Foreground,
        ..spec(home)
    };
    let mut child = Custody::spawn(run, write, cx).unwrap();
    let exit = *child.wait(op).verified().unwrap();
    assert_eq!(exit.code, 0);
    child.complete(run, exit).unwrap();
    assert_eq!(child.tail(), b"first-captured");
    // Release-on-exit: the lease ended with the workload, not with settle.
    assert_owner_terminal(home, "first-done");
    wait_terminal_observation(op, home, "first-observed");

    let read = ProcessSpec {
        argv: vec![
            "/bin/sh".into(),
            "-c".into(),
            "IFS= read -r line < /dev/tty && printf 'second-ok:%s' \"$line\"".into(),
        ],
        terminal: Terminal::Foreground,
        ..spec(home)
    };
    let mut child = Custody::spawn(run, read, cx).unwrap();
    let exit = *child.wait(op).verified().unwrap();
    assert_eq!(exit.code, 0);
    child.complete(run, exit).unwrap();
    assert_eq!(child.tail(), b"second-ok:terminal-input");
    assert_owner_terminal(home, "second-done");
    wait_terminal_observation(op, home, "second-observed");
}

// Hold this exact terminal phase until the parent observes it: otherwise
// the next workload or session exit races tcgetpgrp in the proof process.
fn wait_terminal_observation(op: &OperationContext, home: &Path, marker: &str) {
    while !home.join(marker).exists() {
        op.check("terminal phase observation").unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The engine payload a fixture client hands its supervisor: the home.
fn engine_launch(home: &Path) -> EngineLaunch {
    EngineLaunch {
        payload: home.as_os_str().as_encoded_bytes().to_vec(),
        cwd: home.into(),
        env: vec![("FIXTURE_ENGINE_ENV".into(), "present".into())],

        handoff: None,
        admission: None,
    }
}

/// A minimal engine supervisor: it admits its own run, captures its output,
/// runs one workload, echoes client frames and stops on its owner channel.
fn fixture_engine(mut session: EngineSession, mut router: RouterHandle) -> Result<(), RunError> {
    let home = std::path::PathBuf::from(
        std::str::from_utf8(session.payload()).map_err(|_| RunError::broken("fixture payload"))?,
    );
    let op = OperationContext::new(
        Deadline::Open(OpenReason::DevServer),
        router.root_cancel(),
        op(CancelToken::default()).limits().clone(),
        EffectClass::Mutate,
        effigy_kernel::ctx::Placement::Host,
    );
    let cx = op.effects().unwrap();
    let cleanup = op.cleanup(Duration::from_secs(10)).unwrap();
    let root = StateRoot::effects(&cx, &home).unwrap();
    let opts = BeginOpts {
        id: RunId::new(1, [9; 10]).unwrap(),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: Some(LifecyclePlan {
            executable: Executable::current().unwrap(),
            placement: effigy_kernel::supervise::Placement::Detached,
        }),
    };
    // A dispatcher that reserved a keyed workspace request hands the ticket to
    // this detached supervisor; admitting under it is the same `begin` with the
    // binding committed in its one admission transaction (052 §8.11).
    let begun = match session.admission().cloned() {
        Some(ticket) => Run::begin_admitted(
            &cx,
            &root,
            &invocation(),
            opts,
            &cleanup,
            &mut Strict,
            ticket,
        ),
        None => Run::begin(&cx, &root, &invocation(), opts, &cleanup, &mut Strict),
    };
    let mut run = match begun {
        Ok(run) => run,
        Err(error) => {
            session.refused_message(error.category(), error.to_string());
            return Err(error);
        }
    };
    session.admitted(&mut run)?;
    let secrets = if std::env::var_os("FIXTURE_CAPTURE_EXHAUSTED").is_some() {
        (0x20u8..=0x7e).map(|byte| vec![byte]).collect()
    } else {
        vec![b"engine-secret".to_vec()]
    };
    let capture = || session.capture_output(&mut run, secrets);
    #[cfg(feature = "fault-injection")]
    let captured = if let Ok(point) = std::env::var("FIXTURE_CAPTURE_SETUP_FAULT") {
        effigy_kernel::fault_injection::with_hook(
            move |seen| {
                if seen == point {
                    Err(std::io::Error::from_raw_os_error(libc::ENOSPC))
                } else {
                    Ok(())
                }
            },
            capture,
        )
    } else {
        capture()
    };
    #[cfg(not(feature = "fault-injection"))]
    let captured = capture();
    captured.map_err(RunError::from)?;
    println!(
        "engine output engine-secret cwd={} env={}",
        std::env::current_dir().unwrap().display(),
        std::env::var("FIXTURE_ENGINE_ENV").unwrap_or_default()
    );
    let mut child = Custody::spawn(
        &mut run,
        ProcessSpec {
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                if std::env::var_os("FIXTURE_ENGINE_LONG_GRACE").is_some() {
                    "trap '' TERM; echo $$ > engine-workload.pid; exec /bin/sleep 40".into()
                } else {
                    "echo $$ > engine-workload.pid; exec /bin/sleep 40".into()
                },
            ],
            grace: if std::env::var_os("FIXTURE_ENGINE_LONG_GRACE").is_some() {
                Duration::from_secs(10)
            } else {
                Duration::from_millis(25)
            },
            ..spec(&home)
        },
        &cx,
    )
    .map_err(|error| RunError::unavailable(format!("the fixture workload spawn: {error:?}")))?;
    run.advance(Phase::Active)?;
    session.active();
    let cause = loop {
        let routed = router.poll();
        let control = session.control(&mut run, &routed)?;
        for signal in &control.forward {
            child.forward(*signal);
        }
        for frame in control.frames {
            if frame == b"confirm-detached" {
                session.event(vec![u8::from(session.detached())]);
                continue;
            }
            if frame == b"resume-secondary" {
                let key = session.admission().expect("workspace ticket").key();
                let attachment = resume_secondary(
                    &cx,
                    &root,
                    key.session,
                    run.id(),
                    AttachmentId::new([8; 16]),
                )
                .inspect_err(|error| {
                    session.event(format!("secondary attach: {error}").into_bytes())
                })?;
                let socket = std::os::unix::net::UnixStream::connect(home.join("a.sock"))
                    .inspect_err(|error| {
                        session.event(format!("secondary socket: {error}").into_bytes())
                    })?;
                let input = std::fs::File::from(std::os::fd::OwnedFd::from(socket));
                session
                    .watch_attachment(&run, &attachment, &input)
                    .inspect_err(|error| {
                        session.event(format!("secondary watch: {error}").into_bytes())
                    })?;
            }
            session.event(frame);
        }
        if let Some(cause) = control.stop {
            break cause;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    std::fs::write(home.join("engine-stop-cause"), format!("{cause:?}")).unwrap();
    run.interrupt(
        &cleanup,
        WorkloadOutcome::Cancelled {
            signal: match &cause {
                StopCause::Signal(signal) => Some(*signal),
                _ => None,
            },
            cause,
        },
    )?;
    let outcome = run.settle(&cleanup, &mut Strict)?;
    session.closed(&outcome);
    Ok(())
}

#[path = "probe-fixture.rs"]
mod probe_fixture;
