use std::collections::BTreeSet;
use std::io;
use std::process::Child;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

static CURRENT_SIGNAL_STATE: OnceLock<Mutex<Option<Arc<Mutex<SignalState>>>>> = OnceLock::new();

/// Signal-forwarding scope for a process that owns task children. Signals are
/// sent only to process groups started inside the scope.
pub(super) struct OwnedChildrenScope {
    previous_signal_state: Option<Arc<Mutex<SignalState>>>,
    previous_observer: Option<effigy_process::ProcessGroupObserver>,
    _signal_forwarder: SignalForwarder,
}

impl OwnedChildrenScope {
    pub(super) fn enter() -> io::Result<Self> {
        let signal_forwarder = SignalForwarder::install()?;
        let previous_signal_state =
            replace_current_signal_state(Some(signal_forwarder.state.clone()));
        let signal_state = signal_forwarder.state.clone();
        let observer = Arc::new(move |event| match event {
            effigy_process::ProcessGroupEvent::Started(pid) => {
                signal_state_register(&signal_state, pid);
            }
            effigy_process::ProcessGroupEvent::Stopped(pid) => {
                signal_state_unregister_if_gone(&signal_state, pid);
            }
        }) as effigy_process::ProcessGroupObserver;
        let previous_observer = effigy_process::replace_process_group_observer(Some(observer));
        Ok(Self {
            previous_signal_state,
            previous_observer,
            _signal_forwarder: signal_forwarder,
        })
    }
}

impl Drop for OwnedChildrenScope {
    fn drop(&mut self) {
        effigy_process::replace_process_group_observer(self.previous_observer.take());
        replace_current_signal_state(self.previous_signal_state.take());
    }
}

pub(super) fn signal_scope_active() -> bool {
    current_signal_state().is_some()
}

pub(super) fn register_process_group(pid: u32) {
    if let Some(state) = current_signal_state() {
        signal_state_register(&state, pid);
    }
}

pub(super) fn unregister_process_group(pid: u32) {
    if let Some(state) = current_signal_state() {
        signal_state_unregister_if_gone(&state, pid);
    }
}

/// Put the child in a new Unix process group so timeout or cancellation can
/// target that tree without signalling the caller group. Non-Unix is a no-op.
pub(super) fn configure_owned_process_group(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

/// Result of waiting on a child that this process spawned into an owned group.
#[derive(Debug)]
pub(super) enum OwnedChildOutcome {
    Exited(std::process::ExitStatus),
    TimedOut,
}

/// Poll until the child exits or `deadline` is reached. On timeout the owned
/// group is stopped and the direct child is reaped. Drop also terminates an
/// unfinished child so error paths cannot leak the tree.
pub(super) fn wait_for_owned_child(
    child: Child,
    deadline: Instant,
    poll: Duration,
) -> io::Result<OwnedChildOutcome> {
    OwnedChildSession::new(child).wait_until(deadline, poll)
}

/// Wait until the owned child exits. Drop still terminates the tree if this
/// wait is abandoned.
pub(super) fn wait_for_owned_child_unbounded(child: Child) -> io::Result<std::process::ExitStatus> {
    OwnedChildSession::new(child).wait()
}

struct OwnedChildSession {
    child: Option<Child>,
    pid: u32,
    finished: bool,
}

impl OwnedChildSession {
    fn new(child: Child) -> Self {
        let pid = child.id();
        register_process_group(pid);
        Self {
            child: Some(child),
            pid,
            finished: false,
        }
    }

    fn wait(mut self) -> io::Result<std::process::ExitStatus> {
        let status = {
            let child = self
                .child
                .as_mut()
                .ok_or_else(|| io::Error::other("owned child session lost its process"))?;
            child.wait()?
        };
        self.mark_finished();
        Ok(status)
    }

    fn wait_until(mut self, deadline: Instant, poll: Duration) -> io::Result<OwnedChildOutcome> {
        let outcome = loop {
            {
                let child = self
                    .child
                    .as_mut()
                    .ok_or_else(|| io::Error::other("owned child session lost its process"))?;
                if let Some(status) = child.try_wait()? {
                    break OwnedChildOutcome::Exited(status);
                }
                if Instant::now() >= deadline {
                    terminate_owned_child_tree(child);
                    let _ = child.wait();
                    break OwnedChildOutcome::TimedOut;
                }
            }
            thread::sleep(poll);
        };
        self.mark_finished();
        Ok(outcome)
    }

    fn mark_finished(&mut self) {
        self.finished = true;
        unregister_process_group(self.pid);
    }
}

impl Drop for OwnedChildSession {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Some(mut child) = self.child.take() {
            terminate_owned_child_tree(&mut child);
            let _ = child.wait();
        }
        unregister_process_group(self.pid);
    }
}

fn terminate_owned_child_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        terminate_owned_unix_tree(child);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

/// Signal the child's process group (pgid == child pid after
/// `process_group(0)`). Never group-signals the caller. Non-group fallback is
/// pid-only `Child::kill`.
#[cfg(unix)]
fn terminate_owned_unix_tree(child: &mut Child) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::{getpgrp, Pid};

    let pid = child.id() as i32;
    if pid <= 1 {
        let _ = child.kill();
        return;
    }

    #[cfg(all(test, unix))]
    if group_cleanup_disabled_for_test() {
        let _ = child.kill();
        return;
    }

    let caller_pgid = getpgrp().as_raw();
    if pid == caller_pgid {
        let _ = child.kill();
        return;
    }

    let group = Pid::from_raw(-pid);
    let leader = Pid::from_raw(pid);
    let _ = kill(group, Signal::SIGTERM);
    let _ = kill(leader, Signal::SIGTERM);
    let grace = Instant::now() + Duration::from_millis(800);
    while Instant::now() < grace {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => break,
        }
    }
    let _ = kill(group, Signal::SIGKILL);
    let _ = kill(leader, Signal::SIGKILL);
}

#[derive(Default)]
struct SignalState {
    process_groups: BTreeSet<i32>,
    cancellation_signals: Vec<i32>,
}

struct SignalForwarder {
    state: Arc<Mutex<SignalState>>,
    #[cfg(unix)]
    _listener: SignalListener,
}

fn current_signal_state() -> Option<Arc<Mutex<SignalState>>> {
    CURRENT_SIGNAL_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn replace_current_signal_state(
    state: Option<Arc<Mutex<SignalState>>>,
) -> Option<Arc<Mutex<SignalState>>> {
    let mut current = CURRENT_SIGNAL_STATE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::replace(&mut *current, state)
}

#[cfg(unix)]
struct SignalListener {
    handle: signal_hook::iterator::Handle,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SignalForwarder {
    fn install() -> io::Result<Self> {
        let state = Arc::new(Mutex::new(SignalState::default()));
        #[cfg(unix)]
        let listener = SignalListener::install(Arc::clone(&state))?;
        Ok(Self {
            state,
            #[cfg(unix)]
            _listener: listener,
        })
    }
}

#[cfg(unix)]
impl SignalListener {
    fn install(state: Arc<Mutex<SignalState>>) -> io::Result<Self> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        use signal_hook::iterator::Signals;

        let mut signals = Signals::new([SIGHUP, SIGINT, SIGTERM])?;
        let handle = signals.handle();
        let thread = std::thread::Builder::new()
            .name("effigy-heavy-signal-forwarder".to_owned())
            .spawn(move || {
                for signal in signals.forever() {
                    let process_groups = {
                        let mut state = state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        state
                            .process_groups
                            .retain(|group| process_group_is_live(*group) != Some(false));
                        if !state.cancellation_signals.contains(&signal) {
                            state.cancellation_signals.push(signal);
                        }
                        state.process_groups.iter().copied().collect::<Vec<_>>()
                    };
                    for process_group in process_groups {
                        forward_signal_to_process_group(process_group, signal);
                    }
                }
            })?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}

#[cfg(unix)]
impl Drop for SignalListener {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn signal_state_register(state: &Arc<Mutex<SignalState>>, pid: u32) {
    let process_group = pid as i32;
    let cancellation_signals = {
        let mut state = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .process_groups
            .retain(|group| process_group_is_live(*group) != Some(false));
        if state.process_groups.insert(process_group) {
            state.cancellation_signals.clone()
        } else {
            Vec::new()
        }
    };
    for signal in cancellation_signals {
        forward_signal_to_process_group(process_group, signal);
    }
}

fn signal_state_unregister_if_gone(state: &Arc<Mutex<SignalState>>, pid: u32) {
    if process_group_is_live(pid as i32) == Some(false) {
        state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .process_groups
            .remove(&(pid as i32));
    }
}

fn process_group_is_live(group: i32) -> Option<bool> {
    #[cfg(unix)]
    {
        match nix::sys::signal::kill(nix::unistd::Pid::from_raw(-group), None) {
            Ok(()) => Some(true),
            Err(nix::errno::Errno::ESRCH) => Some(false),
            Err(nix::errno::Errno::EPERM) => Some(true),
            Err(_) => None,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = group;
        None
    }
}

/// Test-only negative seam. While held, the signal forwarder observes signals
/// and records them, but never delivers them to a registered process group.
/// This backs the termination-oracle negative proof: with delivery off the
/// oracle must report no observed termination, and unconditional cleanup must
/// still reap every test-created child. Never compiled into a release build.
#[cfg(test)]
pub(super) struct ForwardingDisabledForTest;

#[cfg(test)]
static FORWARDING_DISABLED_FOR_TEST: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
pub(super) fn disable_forwarding_for_test() -> ForwardingDisabledForTest {
    FORWARDING_DISABLED_FOR_TEST.store(true, std::sync::atomic::Ordering::SeqCst);
    ForwardingDisabledForTest
}

#[cfg(test)]
impl Drop for ForwardingDisabledForTest {
    fn drop(&mut self) {
        FORWARDING_DISABLED_FOR_TEST.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
fn forwarding_disabled_for_test() -> bool {
    FORWARDING_DISABLED_FOR_TEST.load(std::sync::atomic::Ordering::SeqCst)
}

/// Serializes timeout-descendant proofs with the group-cleanup negative seam.
#[cfg(all(test, unix))]
pub(super) fn hold_group_cleanup_test_lock() -> std::sync::MutexGuard<'static, ()> {
    GROUP_CLEANUP_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Test-only negative seam. While held, timeout cleanup kills only the direct
/// child pid and leaves the rest of the owned group running, so the reap
/// oracle must fail. Never compiled into a release build.
#[cfg(all(test, unix))]
pub(super) struct GroupCleanupDisabledForTest {
    _lock: std::sync::MutexGuard<'static, ()>,
}

#[cfg(all(test, unix))]
static GROUP_CLEANUP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(test, unix))]
static GROUP_CLEANUP_DISABLED_FOR_TEST: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(all(test, unix))]
pub(super) fn disable_group_cleanup_for_test() -> GroupCleanupDisabledForTest {
    let lock = hold_group_cleanup_test_lock();
    GROUP_CLEANUP_DISABLED_FOR_TEST.store(true, std::sync::atomic::Ordering::SeqCst);
    GroupCleanupDisabledForTest { _lock: lock }
}

#[cfg(all(test, unix))]
impl Drop for GroupCleanupDisabledForTest {
    fn drop(&mut self) {
        GROUP_CLEANUP_DISABLED_FOR_TEST.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(all(test, unix))]
fn group_cleanup_disabled_for_test() -> bool {
    GROUP_CLEANUP_DISABLED_FOR_TEST.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(unix)]
fn forward_signal_to_process_group(process_group: i32, signal: i32) {
    #[cfg(test)]
    if forwarding_disabled_for_test() {
        return;
    }

    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

    let signal = match signal {
        SIGHUP => Signal::SIGHUP,
        SIGINT => Signal::SIGINT,
        SIGTERM => Signal::SIGTERM,
        _ => return,
    };
    let _ = kill(Pid::from_raw(-process_group), signal);
}

#[cfg(not(unix))]
fn forward_signal_to_process_group(_process_group: i32, _signal: i32) {}

/// Private leader/descendant fixtures for sequence and managed timeout-reap
/// proofs. Readiness is the recorded pid file, never a short sleep.
#[cfg(all(test, unix))]
pub(super) mod timeout_descendant_proof {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    pub(in crate::runner) struct TimeoutDescendantFixture {
        dir: tempfile::TempDir,
        ready: PathBuf,
        sibling: Option<Child>,
    }

    impl TimeoutDescendantFixture {
        pub fn new(prefix: &str) -> Self {
            let dir = tempfile::Builder::new()
                .prefix(prefix)
                .tempdir()
                .expect("tempdir");
            let ready = dir.path().join("ready.pids");
            Self {
                dir,
                ready,
                sibling: None,
            }
        }

        pub fn hang_command(&self) -> String {
            let pending = self.ready.with_extension("pids.pending");
            format!(
                "/bin/sleep 300 &\n descendant=$!\n printf '%s\\n%s\\n' \"$$\" \"$descendant\" > '{pending}'\n mv '{pending}' '{ready}'\n wait\n",
                pending = pending.display(),
                ready = self.ready.display(),
            )
        }

        pub fn spawn_unrelated_sibling(&mut self) {
            use std::os::unix::process::CommandExt;
            let child = Command::new("/bin/sleep")
                .arg("300")
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn unrelated sibling");
            self.adopt_direct_child(child);
        }

        pub fn adopt_direct_child(&mut self, child: Child) {
            self.sibling = Some(child);
        }

        pub fn cwd(&self) -> &Path {
            self.dir.path()
        }

        pub fn sibling_pid(&self) -> i32 {
            self.sibling
                .as_ref()
                .map(|child| child.id() as i32)
                .expect("sibling was not spawned")
        }

        pub fn wait_for_recorded_pids(&self) -> Vec<i32> {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(pids) = self.recorded_pids_if_ready() {
                    return pids;
                }
                assert!(
                    Instant::now() < deadline,
                    "fixture never recorded its leader and descendant at {}",
                    self.ready.display()
                );
                thread::sleep(Duration::from_millis(20));
            }
        }

        fn recorded_pids_if_ready(&self) -> Option<Vec<i32>> {
            let text = fs::read_to_string(&self.ready).ok()?;
            let pids = text
                .lines()
                .filter_map(|line| line.trim().parse::<i32>().ok())
                .filter(|pid| *pid > 1)
                .collect::<Vec<_>>();
            if pids.len() >= 2 {
                Some(pids)
            } else {
                None
            }
        }

        pub fn pid_alive(pid: i32) -> bool {
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
        }

        pub fn assert_owned_gone(&self, pids: &[i32]) {
            let alive = pids
                .iter()
                .copied()
                .filter(|pid| Self::pid_alive(*pid))
                .collect::<Vec<_>>();
            assert!(
                alive.is_empty(),
                "recorded owned leader/descendant still alive: {alive:?}"
            );
        }

        pub fn assert_sibling_alive(&self) {
            let pid = self.sibling_pid();
            assert!(
                Self::pid_alive(pid),
                "unrelated sibling {pid} was signalled"
            );
        }
    }

    impl Drop for TimeoutDescendantFixture {
        fn drop(&mut self) {
            let pids = self.recorded_pids_if_ready().unwrap_or_default();
            let mut extra = pids;
            if let Some(child) = &self.sibling {
                extra.push(child.id() as i32);
            }
            for pid in &extra {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(*pid),
                    nix::sys::signal::Signal::SIGTERM,
                );
            }
            let grace = Instant::now() + Duration::from_secs(2);
            while extra.iter().any(|pid| Self::pid_alive(*pid)) && Instant::now() < grace {
                thread::sleep(Duration::from_millis(20));
            }
            for pid in &extra {
                if Self::pid_alive(*pid) {
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(*pid),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                }
            }
            if let Some(mut child) = self.sibling.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while extra.iter().any(|pid| Self::pid_alive(*pid)) {
                assert!(
                    Instant::now() < deadline,
                    "private fixture guard could not reap recorded pids: {extra:?}"
                );
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
