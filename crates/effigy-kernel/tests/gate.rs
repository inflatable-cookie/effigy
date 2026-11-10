//! §5.4 Gate proofs against real `flock`s and real holder processes.
mod support;
use effigy_kernel::ctx::*;
use effigy_kernel::gate::*;
use effigy_kernel::id::*;
use effigy_kernel::ident::ProcessState;
use effigy_kernel::observe::*;
use effigy_kernel::root::StateRoot;
use effigy_kernel::store::Store;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use support::*;

fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::LinkedWorktree, [7; 16])
}
fn task(name: &str) -> ResourceId {
    TaskId::new(checkout(), "apps/api", Surface::Published, name, None)
        .unwrap()
        .resource()
}
fn root(home: &Path) -> StateRoot {
    StateRoot::effects(&effects(EffectClass::Mutate), home).unwrap()
}
fn observed_root(home: &Path) -> StateRoot {
    match StateRoot::observe(&observe(), home) {
        Observed::Verified(root, _) => root,
        other => panic!("state root not observed: {other:?}"),
    }
}
fn acquire(home: &Path, ids: &[ResourceId], wait: Wait) -> Result<GateGuard, GateError> {
    Gate::acquire(&effects(EffectClass::Mutate), &root(home), ids, wait)
}
fn probe(home: &Path, id: &ResourceId) -> Observed<GateState> {
    Gate::probe(&observe(), &observed_root(home), id)
}
fn spawn_holder(home: &Path, mode: &str) -> Helper {
    let helper = Helper::spawn(
        "helper_gate_holder",
        &[
            ("HOME", home.to_str().unwrap()),
            ("EFFIGY_KERNEL_TEST_MODE", mode),
        ],
    );
    helper.expect("HELD", Duration::from_secs(60));
    helper
}

/// Holds `task("build")` until told to release. In `fork` mode it first
/// spawns a long-lived workload and prints its pid.
#[test]
#[ignore = "helper process"]
fn helper_gate_holder() {
    if !helper_is("helper_gate_holder") {
        return;
    }
    let home = StateRoot::home_from_env().unwrap();
    let guard = acquire(&home, &[task("build")], Wait::FailFast).unwrap();
    if std::env::var("EFFIGY_KERNEL_TEST_MODE").as_deref() == Ok("fork") {
        // The holder is SIGKILLed with the workload still running; init
        // reaps it after the test stops it by its reported pid.
        #[allow(clippy::zombie_processes)]
        let workload = std::process::Command::new("sleep")
            .arg("120")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        println!("WORKLOAD {}", workload.id());
    }
    println!("HELD");
    while let Some(command) = read_command() {
        if command == "release" {
            drop(guard);
            println!("RELEASED");
            return;
        }
    }
}

fn holder_diag(holder: &Observed<HolderDiag>) -> &HolderDiag {
    holder.verified().expect("holder diagnostics readable")
}

/// A concurrent fork kept before exec/descriptor closure until dropped.
/// Only async-signal-safe syscalls run in the child; it never drops Rust guards.
struct PausedFork {
    pid: libc::pid_t,
    barrier: UnixStream,
}
impl PausedFork {
    fn spawn() -> Self {
        Self::spawn_with_closed_controls(false)
    }
    fn spawn_without_control_pipes() -> Self {
        Self::spawn_with_closed_controls(true)
    }
    fn spawn_with_closed_controls(close_controls: bool) -> Self {
        use std::os::unix::fs::FileTypeExt;
        let (parent, child) = UnixStream::pair().unwrap();
        parent
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let fd = child.as_raw_fd();
        let parent_fd = parent.as_raw_fd();
        // Retain regular gate files, but not copies of the memory guard's EOF
        // sockets/pipes: those would prevent the acknowledged guard closing
        // and test a different, correctly refused custody boundary.
        let controls: Vec<i32> = if close_controls {
            std::fs::read_dir("/dev/fd")
                .unwrap()
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let raw = entry.file_name().to_str()?.parse::<i32>().ok()?;
                    let kind = std::fs::metadata(entry.path()).ok()?.file_type();
                    (raw > 2
                        && raw != fd
                        && raw != parent_fd
                        && (kind.is_fifo() || kind.is_socket()))
                    .then_some(raw)
                })
                .collect()
        } else {
            Vec::new()
        };
        // SAFETY: the child runs only the syscall-only branch below and _exit;
        // it neither allocates nor accesses locks inherited from other threads.
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0, "fork failed: {}", std::io::Error::last_os_error());
        if pid == 0 {
            // SAFETY: fd is a live inherited socket; the one-byte buffer and
            // pollfd are initialized stack storage. No Rust destructor runs.
            unsafe {
                libc::close(parent_fd);
                for &control in &controls {
                    libc::close(control);
                }
                let ready = [1u8];
                if libc::write(fd, ready.as_ptr().cast(), 1) != 1 {
                    libc::_exit(125);
                }
                let mut poll = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                // Bound the child even if its parent panics or disappears.
                libc::poll(&mut poll, 1, 30_000);
                libc::_exit(0);
            }
        }
        let mut child_guard = Self {
            pid,
            barrier: parent,
        };
        let mut ready = [0u8];
        child_guard.barrier.read_exact(&mut ready).unwrap();
        assert_eq!(ready, [1]);
        child_guard
    }
    fn assert_running(&self) {
        assert_eq!(
            effigy_kernel::ident::process_state(self.pid as u32).verified(),
            Some(&ProcessState::Active),
            "fork child exited before the release proof"
        );
    }
}
impl Drop for PausedFork {
    fn drop(&mut self) {
        // SAFETY: this is our unreaped direct child, so its pid cannot be reused.
        // Kill only it, then reap; EINTR does not transfer wait ownership.
        unsafe {
            libc::kill(self.pid, libc::SIGKILL);
            while libc::waitpid(self.pid, std::ptr::null_mut(), 0) < 0 {
                if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
}

fn foreground_run(
    cx: &EffectContext,
    root: &StateRoot,
    name: &str,
    n: u8,
    parent: Option<&mut effigy_kernel::run::ForegroundRun>,
) -> effigy_kernel::run::ForegroundRun {
    use effigy_kernel::{outcome::Invocation, run::*, supervise::*};
    let task = TaskId::new(checkout(), "apps/api", Surface::Published, name, None).unwrap();
    let invocation = ResolvedInvocation::new(InvocationSpec {
        owner: task.resource(),
        task,
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: Default::default(),
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: effigy_kernel::ctx::Placement::Host,
        provenance: Default::default(),
    })
    .unwrap();
    let opts = BeginOpts {
        id: RunId::new(1, [n; 10]).unwrap(),
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: Invocation::validated(vec!["recover".into()], |a| a == ["recover"]).unwrap(),
        custody: Some(LifecyclePlan {
            executable: Executable::capture(Path::new(env!(
                "CARGO_BIN_EXE_kernel-custody-fixture"
            )))
            .unwrap(),
            placement: effigy_kernel::supervise::Placement::Inline,
        }),
    };
    match parent {
        Some(parent) => parent.begin_child(root, &invocation, opts),
        None => ForegroundRun::begin(cx, root, &invocation, opts),
    }
    .unwrap()
}

#[test]
fn settled_foreground_gate_releases_while_an_unrelated_fork_keeps_its_descriptors() {
    use effigy_kernel::outcome::{CleanupOutcome, WorkloadOutcome};
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut run = foreground_run(&cx, &root, "foreground", 1, None);
    // This models a sibling thread's fork paused before its descriptor sweep.
    // Its inherited copy is unrelated to this run's acknowledged memory guard.
    let fork = std::thread::spawn(PausedFork::spawn_without_control_pipes)
        .join()
        .unwrap();
    let id = task("foreground");
    assert!(matches!(
        probe(home.path(), &id),
        Observed::Verified(GateState::Held(_), _)
    ));
    run.finish(WorkloadOutcome::Failed { code: 7 }).unwrap();
    let settled = run
        .settle(&cx.operation().cleanup(Duration::from_secs(10)).unwrap())
        .unwrap();
    assert_eq!(settled.cleanup(), CleanupOutcome::Complete);
    fork.assert_running();
    assert!(
        matches!(
            probe(home.path(), &id),
            Observed::Verified(GateState::Free, _)
        ),
        "a completed foreground run must not be retained by a pre-exec fork copy"
    );
    let successor = acquire(home.path(), std::slice::from_ref(&id), Wait::FailFast).unwrap();
    drop(fork);
    assert!(matches!(
        probe(home.path(), &id),
        Observed::Verified(GateState::Held(_), _)
    ));
    drop(successor);
}

#[test]
fn unsettled_foreground_descendant_retains_ancestor_gate_after_parent_failure() {
    use effigy_kernel::outcome::{CleanupOutcome, WorkloadOutcome};
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = StateRoot::effects(&cx, home.path()).unwrap();
    let mut parent = foreground_run(&cx, &root, "parent", 1, None);
    let mut child = foreground_run(&cx, &root, "child", 2, Some(&mut parent));
    let mut descendant = foreground_run(&cx, &root, "descendant", 3, Some(&mut child));
    let fork = std::thread::spawn(PausedFork::spawn_without_control_pipes)
        .join()
        .unwrap();
    let cleanup = cx.operation().cleanup(Duration::from_secs(10)).unwrap();
    child.finish(WorkloadOutcome::Failed { code: 7 }).unwrap();
    assert_eq!(
        child.settle(&cleanup).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
    parent.finish(WorkloadOutcome::Failed { code: 7 }).unwrap();
    assert_eq!(
        parent.settle(&cleanup).unwrap().cleanup(),
        CleanupOutcome::Complete
    );
    // Remove the incidental copy: only the legitimate descendant's custody
    // can retain these ancestor descriptions now.
    drop(fork);
    for name in ["parent", "child"] {
        assert!(
            matches!(
                probe(home.path(), &task(name)),
                Observed::Verified(GateState::Held(_), _)
            ),
            "{name} must remain held until its unconfirmed descendant closes"
        );
    }
    descendant.finish(WorkloadOutcome::Succeeded).unwrap();
    descendant.settle(&cleanup).unwrap();
    for name in ["parent", "child"] {
        assert!(matches!(
            probe(home.path(), &task(name)),
            Observed::Verified(GateState::Free, _)
        ));
    }
}

#[test]
fn gate_release_is_immediate_while_a_concurrent_fork_keeps_its_descriptors() {
    let home = home();
    let id = task("build");
    let guard = acquire(home.path(), std::slice::from_ref(&id), Wait::FailFast).unwrap();
    let child = std::thread::spawn(PausedFork::spawn).join().unwrap();
    let held = probe(home.path(), &id);
    assert!(
        matches!(&held, Observed::Verified(GateState::Held(diag), _)
        if holder_diag(diag).pid == std::process::id()),
        "before drop: {held:?}"
    );
    drop(guard);
    let released = probe(home.path(), &id);
    child.assert_running();
    assert!(
        matches!(released, Observed::Verified(GateState::Free, _)),
        "after drop with fork child {} still before exec: {released:?}",
        child.pid
    );
    // A successor can acquire while the child still has the old description.
    let successor = acquire(home.path(), &[id], Wait::FailFast).unwrap();
    child.assert_running();
    drop(child);
    assert!(matches!(
        probe(home.path(), &task("build")),
        Observed::Verified(GateState::Held(_), _)
    ));
    drop(successor);
}

#[test]
fn gate_rollback_releases_resource_and_scope_while_a_concurrent_fork_keeps_them() {
    let home = home();
    let blocked = ResourceId::store(Scope::Checkout(checkout()), "graph").unwrap();
    let _held = acquire(home.path(), std::slice::from_ref(&blocked), Wait::FailFast).unwrap();
    let first = ResourceId::store(Scope::Machine, "vault").unwrap();
    assert!(first.canonical() < blocked.canonical());
    let cancel = CancelToken::default();
    let cx = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(120)),
        cancel.clone(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap();
    let state = root(home.path());
    let ids = [first.clone(), blocked];
    let waiter = std::thread::spawn(move || {
        Gate::acquire(
            &cx,
            &state,
            &ids,
            Wait::Until(Instant::now() + Duration::from_secs(60)),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if matches!(
            probe(home.path(), &first),
            Observed::Verified(GateState::Held(_), _)
        ) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "waiter never took its first gate"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    let child = std::thread::spawn(PausedFork::spawn).join().unwrap();
    cancel.cancel(StopCause::Requested);
    assert!(matches!(
        waiter.join().unwrap(),
        Err(GateError::Interrupted(_))
    ));
    let released = probe(home.path(), &first);
    assert!(
        matches!(released, Observed::Verified(GateState::Free, _)),
        "rollback with fork child {}: {released:?}",
        child.pid
    );
    // A leaked shared scope lock would still block scope removal even if the
    // exclusive resource lock was correctly released.
    let scope = std::fs::File::open(root(home.path()).path().join("gates/machine.scope")).unwrap();
    // SAFETY: live, independently opened scope fd; a nonblocking lock attempt.
    let locked = unsafe { libc::flock(scope.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    // SAFETY: release only the lock on this test's independent description.
    unsafe { libc::flock(scope.as_raw_fd(), libc::LOCK_UN) };
    child.assert_running();
    assert_eq!(
        locked, 0,
        "rollback retained the scope lock in child {}",
        child.pid
    );
}

#[test]
fn gate_sigstop_holder_is_never_stolen_and_completes_on_resume() {
    let home = home();
    let mut holder = spawn_holder(home.path(), "plain");
    holder.signal(libc::SIGSTOP);
    // Observe the real stop before testing contention and the wait deadline.
    let stopped_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match acquire(home.path(), &[task("build")], Wait::FailFast) {
            Err(GateError::Busy { id, holder: diag }) => {
                assert_eq!(id, task("build"));
                let diag = holder_diag(&diag);
                assert_eq!(diag.pid, holder.pid());
                if diag.state.verified() == Some(&ProcessState::Stopped) {
                    break;
                }
            }
            other => panic!("stopped holder's gate was granted: {other:?}"),
        }
        assert!(Instant::now() < stopped_deadline, "holder never stopped");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        probe(home.path(), &task("build")),
        Observed::Verified(GateState::Held(_), _)
    ));
    // Prepare the filesystem/context before starting the wait budget. `waited`
    // starts inside Gate::acquire, whereas Until is an absolute caller deadline.
    // Comparing waited with the whole budget incorrectly counts preparation
    // time (and even call overhead), which is visible under shared CI load.
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let ids = [task("build")];
    let start = Instant::now();
    let deadline = start + Duration::from_secs(2);
    match Gate::acquire(&cx, &root, &ids, Wait::Until(deadline)) {
        Err(GateError::TimedOut {
            waited,
            holder: diag,
            ..
        }) => {
            let returned = Instant::now();
            assert!(returned >= deadline, "gate timed out before its deadline");
            assert!(waited > Duration::ZERO && waited <= returned - start);
            assert_eq!(
                holder_diag(&diag).state.verified(),
                Some(&ProcessState::Stopped)
            );
        }
        other => panic!("expected a timed-out wait: {other:?}"),
    }
    assert!(start.elapsed() < Duration::from_secs(10));
    holder.signal(libc::SIGCONT);
    holder.send("release");
    holder.expect("RELEASED", Duration::from_secs(30));
    assert!(holder.wait().success());
    acquire(home.path(), &[task("build")], Wait::FailFast).unwrap();
}

#[test]
fn gate_sigkill_holder_frees_the_gate_at_once_and_its_run_reads_interrupted() {
    let home = home();
    let holder = spawn_holder(home.path(), "plain");
    let held = probe(home.path(), &task("build"));
    assert!(matches!(held, Observed::Verified(GateState::Held(_), _)));
    let live = owner_state(
        Observed::Verified(
            RunPhaseFact::NonClosed,
            Evidence::new("row".into(), None).unwrap(),
        ),
        held,
    );
    assert_eq!(live.verified(), Some(&OwnerState::Held));
    holder.signal(libc::SIGKILL);
    assert!(!holder.wait().success());
    let start = Instant::now();
    let guard = acquire(home.path(), &[task("build")], Wait::FailFast).unwrap();
    assert!(start.elapsed() < FAIL_FAST);
    drop(guard);
    // D4: the free gate plus the predecessor's non-closed row is Interrupted.
    let interrupted = owner_state(
        Observed::Verified(
            RunPhaseFact::NonClosed,
            Evidence::new("row".into(), None).unwrap(),
        ),
        probe(home.path(), &task("build")),
    );
    assert_eq!(interrupted.verified(), Some(&OwnerState::Interrupted));
    assert!(matches!(
        owner_state(
            Observed::Verified(
                RunPhaseFact::NonClosed,
                Evidence::new("row".into(), None).unwrap()
            ),
            Observed::Unknown(UnknownReason::Injected),
        ),
        Observed::Unknown(UnknownReason::Injected)
    ));
}

#[test]
fn gate_forked_workload_never_holds_a_gate_after_the_owner_dies() {
    let home = home();
    let mut holder = Helper::spawn(
        "helper_gate_holder",
        &[
            ("HOME", home.path().to_str().unwrap()),
            ("EFFIGY_KERNEL_TEST_MODE", "fork"),
        ],
    );
    let workload: i32 = holder
        .expect("WORKLOAD", Duration::from_secs(60))
        .parse()
        .unwrap();
    holder.expect("HELD", Duration::from_secs(60));
    holder.signal(libc::SIGKILL);
    assert!(!holder.wait_for_exit().success());
    // SAFETY: signal 0 only checks that the workload this test caused lives.
    assert_eq!(
        unsafe { libc::kill(workload, 0) },
        0,
        "workload still alive"
    );
    let acquired = acquire(home.path(), &[task("build")], Wait::FailFast);
    // SAFETY: stops the workload the helper reported having spawned.
    unsafe { libc::kill(workload, libc::SIGKILL) };
    acquired.expect("a gate descriptor leaked into the workload");
}

#[test]
fn gate_guest_acquire_is_refused_without_touching_the_state_root() {
    let home = home();
    let before = snapshot(home.path());
    let guest = operation(
        EffectClass::Mutate,
        Placement::Guest,
        Duration::from_secs(5),
    )
    .effects()
    .unwrap();
    assert_eq!(
        StateRoot::effects(&guest, home.path()).unwrap_err(),
        effigy_kernel::fs::FsError::HostAuthorityRequired
    );
    let guest_observe = operation(
        EffectClass::Observe,
        Placement::Guest,
        Duration::from_secs(5),
    )
    .observe()
    .unwrap();
    assert!(matches!(
        StateRoot::observe(&guest_observe, home.path()),
        Observed::Unknown(UnknownReason::HostAuthorityRequired)
    ));
    let host_root = root(home.path());
    let before_acquire = snapshot(home.path());
    assert_eq!(
        Gate::acquire(&guest, &host_root, &[task("build")], Wait::FailFast).unwrap_err(),
        GateError::HostAuthorityRequired
    );
    assert_eq!(snapshot(home.path()), before_acquire);
    assert_ne!(before, before_acquire, "host open created the root");
}

#[test]
fn gate_probe_is_pure_and_reports_never_acquired_held_and_free() {
    let home = home();
    let _root = root(home.path());
    let before = snapshot(home.path());
    assert!(matches!(
        probe(home.path(), &task("build")),
        Observed::Absent(AbsenceProof::NoGateInode)
    ));
    assert_eq!(snapshot(home.path()), before, "probe wrote state");
    let guard = acquire(home.path(), &[task("build")], Wait::FailFast).unwrap();
    let before = snapshot(home.path());
    match probe(home.path(), &task("build")) {
        Observed::Verified(GateState::Held(diag), _) => {
            let diag = holder_diag(&diag);
            assert_eq!(diag.pid, std::process::id());
            assert_eq!(diag.state.verified(), Some(&ProcessState::Active));
        }
        other => panic!("held gate probed as {other:?}"),
    }
    let run = RunId::new(1, [1; 10]).unwrap();
    guard.set_run(&run);
    if let Observed::Verified(GateState::Held(diag), _) = probe(home.path(), &task("build")) {
        assert_eq!(
            holder_diag(&diag).run.as_deref(),
            Some(run.to_string().as_str())
        );
    }
    let after_body = snapshot(home.path());
    drop(guard);
    let released = probe(home.path(), &task("build"));
    assert!(
        matches!(released, Observed::Verified(GateState::Free, _)),
        "after guard drop: {released:?}"
    );
    // Probing changed nothing: only the holder's own body rewrite differs.
    assert_eq!(before.len(), after_body.len());
    assert_eq!(snapshot(home.path()), after_body);
    // A held gate's probe never blocks the holder's release or reacquire.
    acquire(home.path(), &[task("build")], Wait::FailFast).unwrap();
}

#[test]
fn gate_acquisition_is_canonical_order_and_all_or_nothing() {
    let home = home();
    let (a, b) = (task("a"), task("b"));
    let machine = ResourceId::store(Scope::Machine, "vault").unwrap();
    let held = acquire(home.path(), std::slice::from_ref(&b), Wait::FailFast).unwrap();
    match acquire(
        home.path(),
        &[a.clone(), b.clone(), machine.clone()],
        Wait::FailFast,
    ) {
        Err(GateError::Busy { id, .. }) => assert_eq!(id, b),
        other => panic!("expected busy: {other:?}"),
    }
    // Nothing taken by the failed attempt is still held.
    assert!(matches!(
        probe(home.path(), &a),
        Observed::Verified(GateState::Free, _)
    ));
    assert!(matches!(
        probe(home.path(), &machine),
        Observed::Verified(GateState::Free, _) | Observed::Absent(_)
    ));
    drop(held);
    // Opposite request orders never deadlock: both sort to one order.
    let homes = home.path().to_path_buf();
    let threads: Vec<_> = [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]]
        .into_iter()
        .map(|ids| {
            let home = homes.clone();
            std::thread::spawn(move || {
                for _ in 0..200 {
                    let guard = acquire(
                        &home,
                        &ids,
                        Wait::Until(Instant::now() + Duration::from_secs(20)),
                    )
                    .unwrap();
                    assert_eq!(guard.ids().count(), 2);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let duplicate = acquire(home.path(), &[a.clone(), a.clone()], Wait::FailFast).unwrap();
    assert_eq!(duplicate.ids().count(), 1);
}

#[test]
fn gate_class_capabilities_bound_which_gates_a_context_may_take() {
    let home = home();
    let root = root(home.path());
    let store = ResourceId::store(Scope::Checkout(checkout()), "graph").unwrap();
    for class in [EffectClass::RefreshDerived, EffectClass::Prepare] {
        let cx = effects(class);
        Gate::acquire(&cx, &root, std::slice::from_ref(&store), Wait::FailFast).unwrap();
        assert!(matches!(
            Gate::acquire(&cx, &root, &[task("build")], Wait::FailFast),
            Err(GateError::ClassRefused { .. })
        ));
    }
    Gate::acquire(
        &effects(EffectClass::Mutate),
        &root,
        &[task("build")],
        Wait::FailFast,
    )
    .unwrap();
}

#[test]
fn gate_waits_are_bounded_by_deadline_and_cancellation() {
    let home = home();
    let _held = acquire(home.path(), &[task("build")], Wait::FailFast).unwrap();
    let start = Instant::now();
    let short = operation(
        EffectClass::Mutate,
        Placement::Host,
        Duration::from_millis(300),
    )
    .effects()
    .unwrap();
    let root = root(home.path());
    match Gate::acquire(
        &short,
        &root,
        &[task("build")],
        Wait::Until(Instant::now() + Duration::from_secs(60)),
    ) {
        Err(GateError::TimedOut { waited, .. }) => assert!(waited < Duration::from_secs(5)),
        other => panic!("context deadline ignored: {other:?}"),
    }
    assert!(start.elapsed() < Duration::from_secs(5));
    let cancel = CancelToken::default();
    let cx = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(60)),
        cancel.clone(),
        limits(),
        EffectClass::Mutate,
        Placement::Host,
    )
    .effects()
    .unwrap();
    cancel.cancel(StopCause::Requested);
    assert!(matches!(
        Gate::acquire(
            &cx,
            &root,
            &[task("build")],
            Wait::Until(Instant::now() + Duration::from_secs(60)),
        ),
        Err(GateError::Interrupted(Interrupt::Cancelled { .. }))
    ));
}

#[test]
fn gate_untrusted_roots_refuse_mutate_and_read_unknown() {
    // A symlinked component.
    let home = home();
    let elsewhere = home.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::create_dir(home.path().join(".effigy")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, home.path().join(".effigy/state")).unwrap();
    assert!(matches!(
        StateRoot::effects(&effects(EffectClass::Mutate), home.path()),
        Err(effigy_kernel::fs::FsError::Untrusted { .. })
    ));
    assert!(matches!(
        StateRoot::observe(&observe(), home.path()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);

    // A group-writable state root.
    let home = self::home();
    let state = root(home.path()).path().to_path_buf();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(matches!(
        StateRoot::effects(&effects(EffectClass::Mutate), home.path()),
        Err(effigy_kernel::fs::FsError::Untrusted { .. })
    ));
    assert!(matches!(
        StateRoot::observe(&observe(), home.path()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));

    // A permission-denied root: Observe succeeds with Unknown, writes nothing.
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o000)).unwrap();
    let before = snapshot(home.path());
    assert!(matches!(
        StateRoot::observe(&observe(), home.path()),
        Observed::Unknown(UnknownReason::PermissionDenied | UnknownReason::Untrusted(_))
    ));
    assert_eq!(snapshot(home.path()), before);
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn gate_effects_adopt_a_legacy_v014_state_root_mode() {
    // Papercut 9e5134a4: v0.14 `effigy state` created `.effigy/state/` with the
    // process umask (0755), which Q-010 made the kernel's private root. An
    // effect in that checkout adopts the root by tightening its mode; an
    // observation never repairs what it does not trust.
    let home = home();
    let state = home.path().join(".effigy/state");
    let legacy = state.join("capture-context/acme/uat.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, b"{}\n").unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mode = |path: &Path| {
        std::fs::symlink_metadata(path)
            .expect("stat")
            .permissions()
            .mode()
            & 0o777
    };

    assert_eq!(mode(&state), 0o755);
    assert!(matches!(
        StateRoot::observe(&observe(), home.path()),
        Observed::Unknown(UnknownReason::Untrusted(_))
    ));
    assert_eq!(mode(&state), 0o755, "observation repairs nothing");

    let root = StateRoot::effects(&effects(EffectClass::Mutate), home.path()).unwrap();
    assert_eq!(mode(root.path()), 0o700);
    assert_eq!(mode(&state), 0o700);
    assert!(
        legacy.is_file(),
        "adopting the mode keeps the legacy content"
    );
    assert_eq!(
        StateRoot::observe(&observe(), home.path()).map(|root| mode(root.path())),
        Observed::Verified(
            0o700,
            Evidence::new("kernel state root".into(), None).unwrap()
        )
    );

    // A root another user could write is still refused, never repaired.
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(matches!(
        StateRoot::effects(&effects(EffectClass::Mutate), home.path()),
        Err(effigy_kernel::fs::FsError::Untrusted { .. })
    ));
}

#[test]
fn gate_acquire_inside_a_store_transaction_is_refused() {
    let home = home();
    let cx = effects(EffectClass::Mutate);
    let root = root(home.path());
    let mut store = Store::open(&cx, &root).unwrap();
    let refused = store
        .txn(&cx, |_| {
            Ok(Gate::acquire(&cx, &root, &[task("build")], Wait::FailFast).map(|_| ()))
        })
        .unwrap();
    assert_eq!(refused, Err(GateError::InTransaction));
}
