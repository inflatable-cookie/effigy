use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "process_manager/diagnostics.rs"]
mod diagnostics;
#[path = "process_manager/lifecycle.rs"]
mod lifecycle;
#[path = "process_manager/signal.rs"]
mod signal;
#[path = "process_manager/streams.rs"]
mod streams;
#[path = "process_manager/supervisor_control.rs"]
mod supervisor_control;
#[path = "process_manager/supervisor_lookup.rs"]
mod supervisor_lookup;
#[path = "process_manager/supervisor_shutdown.rs"]
mod supervisor_shutdown;

use diagnostics::collect_exit_diagnostics;
const PROCESS_GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub name: String,
    pub run: String,
    pub cwd: PathBuf,
    pub start_after_ms: u64,
    pub pty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEventKind {
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
            specs_map.insert(spec.name.clone(), spec.clone());
            process_map.insert(spec.name.clone(), child);
        }

        Ok(Self {
            processes: Arc::new(Mutex::new(process_map)),
            specs: specs_map,
            events_tx,
            events_rx,
        })
    }

    pub fn next_event_timeout(&self, timeout: Duration) -> Option<ProcessEvent> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    pub fn exit_diagnostics(&self) -> Vec<(String, String)> {
        let process_map = self.processes.lock().expect("process map lock");
        collect_exit_diagnostics(&self.specs, &process_map)
    }
}
