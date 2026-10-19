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
    if let Some(result) = internal_entry(entry, &mut router, &mut Strict, |a| {
        a == [OsString::from("recover")]
    }) {
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
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
    let root = StateRoot::effects(&cx, home).unwrap();
    let id = RunId::new(1, [9; 10]).unwrap();
    let inv = invocation();
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
    } else {
        spec(home)
    };
    let mut child = Custody::spawn(&mut run, workload, &cx).unwrap();
    if entry == "inline-orphan" {
        child.wait(&op);
    }

    std::fs::write(
        home.join("receipt"),
        serde_json::to_vec(&child.identity()).unwrap(),
    )
    .unwrap();
    std::fs::write(home.join("ready"), b"ready").unwrap();
    loop {
        if entry == "inline-orphan" {
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

#[path = "probe-fixture.rs"]
mod probe_fixture;
