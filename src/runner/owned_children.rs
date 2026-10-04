use std::collections::BTreeSet;
use std::io;
use std::sync::{Arc, Mutex, OnceLock};

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
