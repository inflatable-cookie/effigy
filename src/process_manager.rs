use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[path = "process_manager/diagnostics.rs"]
mod diagnostics;
#[path = "process_manager/lifecycle.rs"]
mod lifecycle;
#[path = "process_manager/signal.rs"]
mod signal;
#[path = "process_manager/streams.rs"]
mod streams;

use diagnostics::collect_exit_diagnostics;

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

    pub fn send_input(&self, process: &str, input: &str) -> Result<(), ProcessManagerError> {
        let child = {
            let processes = self.processes.lock().expect("process map lock");
            processes.get(process).cloned()
        };
        let Some(child) = child else {
            return Ok(());
        };
        let mut child = child.lock().expect("child lock");
        let Some(stdin) = child.stdin.as_mut() else {
            return Err(ProcessManagerError::MissingStdio {
                process: process.to_owned(),
            });
        };
        stdin
            .write_all(input.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|error| ProcessManagerError::InputWrite {
                process: process.to_owned(),
                error,
            })
    }

    pub fn terminate_all(&self) {
        let children = {
            let processes = self.processes.lock().expect("process map lock");
            processes.values().cloned().collect::<Vec<_>>()
        };
        for child in children {
            signal::send_kill(&mut child.lock().expect("child lock"));
        }
    }

    pub fn terminate_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        let child = {
            let processes = self.processes.lock().expect("process map lock");
            processes.get(process).cloned()
        }
        .ok_or_else(|| ProcessManagerError::ProcessNotFound {
            process: process.to_owned(),
        })?;
        lifecycle::terminate_child_graceful(&child, Duration::from_millis(800));
        Ok(())
    }

    pub fn restart_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        let spec = self.specs.get(process).cloned().ok_or_else(|| {
            ProcessManagerError::ProcessNotFound {
                process: process.to_owned(),
            }
        })?;
        {
            let processes = self.processes.lock().expect("process map lock");
            if let Some(child) = processes.get(process) {
                lifecycle::terminate_child_graceful(child, Duration::from_millis(800));
            }
        }
        let mut restart_spec = spec;
        restart_spec.start_after_ms = 0;
        let replacement = lifecycle::spawn_process_instance(&restart_spec, &self.events_tx, false)?;
        let mut processes = self.processes.lock().expect("process map lock");
        processes.insert(process.to_owned(), replacement);
        Ok(())
    }

    pub fn terminate_all_graceful(&self, timeout: Duration) {
        self.terminate_all_graceful_with_progress(timeout, |_| {});
    }

    pub fn terminate_all_graceful_with_progress<F>(&self, timeout: Duration, mut on_progress: F)
    where
        F: FnMut(ShutdownProgress),
    {
        on_progress(ShutdownProgress::SendingTerm);
        let children = {
            let processes = self.processes.lock().expect("process map lock");
            processes.values().cloned().collect::<Vec<_>>()
        };
        for child in &children {
            let mut child = child.lock().expect("child lock");
            signal::send_terminate(&mut child);
        }

        on_progress(ShutdownProgress::Waiting);
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let all_exited = children.iter().all(|child| {
                child
                    .lock()
                    .expect("child lock")
                    .try_wait()
                    .ok()
                    .flatten()
                    .is_some()
            });
            if all_exited {
                on_progress(ShutdownProgress::Complete {
                    total: children.len(),
                    forced: 0,
                });
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }

        on_progress(ShutdownProgress::ForceKilling);
        let mut forced = 0usize;
        for child in &children {
            let mut child = child.lock().expect("child lock");
            let still_running = child.try_wait().ok().flatten().is_none();
            if !still_running {
                continue;
            }
            signal::send_kill(&mut child);
            forced += 1;
        }
        on_progress(ShutdownProgress::Complete {
            total: children.len(),
            forced,
        });
    }

    pub fn exit_diagnostics(&self) -> Vec<(String, String)> {
        let process_map = self.processes.lock().expect("process map lock");
        collect_exit_diagnostics(&self.specs, &process_map)
    }
}
