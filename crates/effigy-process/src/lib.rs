use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod diagnostics;
mod identity;
mod lifecycle;
mod locks;
mod signal;
mod streams;
mod supervisor_control;
mod supervisor_lookup;
mod supervisor_shutdown;

use diagnostics::collect_exit_diagnostics;
pub use identity::{
    boot_identity, boot_identity_uncached, compare_boot_identity, legacy_boot_time_identity,
    process_start_identity, process_start_identity_matches, BootIdentityComparison,
};
pub use signal::{process_is_descendant_of, process_is_running, terminate_process_tree};
const PROCESS_GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessGroupEvent {
    Started(u32),
    Stopped(u32),
}

pub type ProcessGroupObserver = Arc<dyn Fn(ProcessGroupEvent) + Send + Sync + 'static>;

static PROCESS_GROUP_OBSERVER: OnceLock<Mutex<Option<ProcessGroupObserver>>> = OnceLock::new();

pub fn replace_process_group_observer(
    observer: Option<ProcessGroupObserver>,
) -> Option<ProcessGroupObserver> {
    let mut active = PROCESS_GROUP_OBSERVER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::replace(&mut *active, observer)
}

pub fn process_group_observer_active() -> bool {
    PROCESS_GROUP_OBSERVER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some()
}

/// Return the process group that currently contains `pid` on Unix.
///
/// This is used by test diagnostics to compare the group requested at spawn
/// time with the child's actual group before it exits. Other platforms report
/// the query as unsupported.
pub fn process_group_id(pid: u32) -> std::io::Result<i32> {
    #[cfg(unix)]
    {
        let pid = i32::try_from(pid).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "process id exceeds the platform pid range",
            )
        })?;
        nix::unistd::getpgid(Some(nix::unistd::Pid::from_raw(pid)))
            .map(|group| group.as_raw())
            .map_err(std::io::Error::from)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "process groups are unavailable on this platform",
        ))
    }
}

pub fn notify_process_group_started(pid: u32) -> bool {
    notify_process_group_event(ProcessGroupEvent::Started(pid))
}

pub fn notify_process_group_stopped(pid: u32) -> bool {
    notify_process_group_event(ProcessGroupEvent::Stopped(pid))
}

fn notify_process_group_event(event: ProcessGroupEvent) -> bool {
    let observer = PROCESS_GROUP_OBSERVER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    if let Some(observer) = observer {
        observer(event);
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub name: String,
    pub run: String,
    pub cwd: PathBuf,
    pub start_after_ms: u64,
    pub shutdown_on_exit: bool,
    pub pty: bool,
    /// Additional environment variables to inject via `Command::env()`.
    /// Used for env-schema resolved values including secrets.
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessEventKind {
    Starting,
    Started,
    StartupFailed,
    Stdout,
    Stderr,
    StdoutChunk,
    StderrChunk,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEvent {
    pub process: String,
    pub kind: ProcessEventKind,
    pub payload: String,
    pub chunk: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum ProcessManagerError {
    Spawn {
        process: String,
        command: String,
        error: std::io::Error,
    },
    MissingStdio {
        process: String,
    },
    InputWrite {
        process: String,
        error: std::io::Error,
    },
    ProcessNotFound {
        process: String,
    },
}

impl std::fmt::Display for ProcessManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessManagerError::Spawn {
                process,
                command,
                error,
            } => write!(
                f,
                "failed to spawn process `{process}` with command `{command}`: {error}"
            ),
            ProcessManagerError::MissingStdio { process } => {
                write!(f, "process `{process}` missing stdin/stdout/stderr pipe")
            }
            ProcessManagerError::InputWrite { process, error } => {
                write!(f, "failed writing input to process `{process}`: {error}")
            }
            ProcessManagerError::ProcessNotFound { process } => {
                write!(f, "process `{process}` not found in managed supervisor")
            }
        }
    }
}

impl std::error::Error for ProcessManagerError {}

pub struct ProcessSupervisor {
    processes: Arc<Mutex<HashMap<String, Arc<Mutex<Child>>>>>,
    specs: HashMap<String, ProcessSpec>,
    events_tx: Sender<ProcessEvent>,
    events_rx: Receiver<ProcessEvent>,
    startup_worker: Option<StartupWorker>,
}

struct StartupWorker {
    cancelled: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
    error: Arc<Mutex<Option<ProcessManagerError>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShutdownProgress {
    SendingTerm,
    Waiting,
    ForceKilling,
    Complete { total: usize, forced: usize },
}

impl ProcessSupervisor {
    pub fn spawn(
        _repo_root: PathBuf,
        processes: Vec<ProcessSpec>,
    ) -> Result<Self, ProcessManagerError> {
        let (events_tx, events_rx) = mpsc::channel::<ProcessEvent>();
        let mut process_map: HashMap<String, Arc<Mutex<Child>>> = HashMap::new();
        let mut specs_map: HashMap<String, ProcessSpec> = HashMap::new();

        for spec in processes {
            let child = lifecycle::spawn_process_instance(&spec, &events_tx, true)?;
            let child_pid = crate::locks::lock_tolerant(&child).id();
            notify_process_group_started(child_pid);
            specs_map.insert(spec.name.clone(), spec.clone());
            process_map.insert(spec.name.clone(), child);
        }

        Ok(Self {
            processes: Arc::new(Mutex::new(process_map)),
            specs: specs_map,
            events_tx,
            events_rx,
            startup_worker: None,
        })
    }

    /// Starts managed children in the configured order without blocking the
    /// caller during per-process start delays.
    pub fn spawn_progressively(_repo_root: PathBuf, processes: Vec<ProcessSpec>) -> Self {
        let (events_tx, events_rx) = mpsc::channel::<ProcessEvent>();
        let process_map = Arc::new(Mutex::new(HashMap::new()));
        let specs_map = processes
            .iter()
            .map(|spec| (spec.name.clone(), spec.clone()))
            .collect::<HashMap<_, _>>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let finished = Arc::new(AtomicBool::new(false));
        let worker_finished = finished.clone();
        let worker_process_map = process_map.clone();
        let worker_events_tx = events_tx.clone();
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        let handle = thread::spawn(move || {
            for spec in processes {
                if wait_for_start_delay(&worker_cancelled, spec.start_after_ms) {
                    break;
                }
                if worker_cancelled.load(Ordering::Acquire) {
                    break;
                }
                let _ = worker_events_tx.send(ProcessEvent {
                    process: spec.name.clone(),
                    kind: ProcessEventKind::Starting,
                    payload: String::new(),
                    chunk: None,
                });
                match lifecycle::spawn_process_instance(&spec, &worker_events_tx, false) {
                    Ok(child) => {
                        let child_pid = crate::locks::lock_tolerant(&child).id();
                        crate::notify_process_group_started(child_pid);
                        crate::locks::lock_tolerant(&worker_process_map)
                            .insert(spec.name.clone(), child);
                        let _ = worker_events_tx.send(ProcessEvent {
                            process: spec.name,
                            kind: ProcessEventKind::Started,
                            payload: String::new(),
                            chunk: None,
                        });
                    }
                    Err(start_error) => {
                        *crate::locks::lock_tolerant(&worker_error) = Some(start_error);
                        let _ = worker_events_tx.send(ProcessEvent {
                            process: spec.name,
                            kind: ProcessEventKind::StartupFailed,
                            payload: String::new(),
                            chunk: None,
                        });
                        break;
                    }
                }
            }
            worker_finished.store(true, Ordering::Release);
        });

        Self {
            processes: process_map,
            specs: specs_map,
            events_tx,
            events_rx,
            startup_worker: Some(StartupWorker {
                cancelled,
                finished,
                handle: Mutex::new(Some(handle)),
                error,
            }),
        }
    }

    pub fn next_event_timeout(&self, timeout: Duration) -> Option<ProcessEvent> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    pub fn take_startup_error(&self) -> Option<ProcessManagerError> {
        self.startup_worker
            .as_ref()
            .and_then(|worker| crate::locks::lock_tolerant(&worker.error).take())
    }

    pub fn startup_finished(&self) -> bool {
        self.startup_worker
            .as_ref()
            .is_none_or(|worker| worker.finished.load(Ordering::Acquire))
    }

    pub(crate) fn cancel_startup(&self) {
        let Some(worker) = &self.startup_worker else {
            return;
        };
        worker.cancelled.store(true, Ordering::Release);
        let handle = crate::locks::lock_tolerant(&worker.handle).take();
        if let Some(handle) = handle {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }

    pub fn exit_diagnostics(&self) -> Vec<(String, String)> {
        let process_map = crate::locks::lock_tolerant(&self.processes);
        collect_exit_diagnostics(&self.specs, &process_map)
    }

    pub fn process_ids(&self) -> Vec<(String, u32)> {
        let process_map = crate::locks::lock_tolerant(&self.processes);
        let mut ids = process_map
            .iter()
            .map(|(name, child)| (name.clone(), crate::locks::lock_tolerant(child).id()))
            .collect::<Vec<_>>();
        ids.sort_by(|left, right| left.0.cmp(&right.0));
        ids
    }
}

impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        self.cancel_startup();
    }
}

fn wait_for_start_delay(cancelled: &AtomicBool, start_after_ms: u64) -> bool {
    if cancelled.load(Ordering::Acquire) {
        return true;
    }
    let delay = Duration::from_millis(start_after_ms);
    let started_at = std::time::Instant::now();
    loop {
        if cancelled.load(Ordering::Acquire) {
            return true;
        }
        let Some(remaining) = delay.checked_sub(started_at.elapsed()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        thread::park_timeout(remaining);
    }
    cancelled.load(Ordering::Acquire)
}

#[cfg(all(test, unix))]
mod postfork_test_alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    pub struct CountingAlloc;

    thread_local! {
        static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
    }

    fn record() {
        let _ = COUNT.try_with(|cell| {
            if let Some(n) = cell.get() {
                cell.set(Some(n.saturating_add(1)));
            }
        });
    }

    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record();
            System.alloc(layout)
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record();
            System.alloc_zeroed(layout)
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout)
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record();
            System.realloc(ptr, layout, new_size)
        }
    }

    pub fn begin() {
        let _ = COUNT.try_with(|cell| {
            let _ = cell.get();
            cell.set(Some(0));
        });
    }

    pub fn take() -> usize {
        COUNT.with(|cell| cell.replace(None).unwrap_or(0))
    }
}

#[cfg(all(test, unix))]
#[global_allocator]
static POSTFORK_TEST_ALLOC: postfork_test_alloc::CountingAlloc = postfork_test_alloc::CountingAlloc;
