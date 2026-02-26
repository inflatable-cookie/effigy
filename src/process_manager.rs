use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    pub name: String,
    pub run: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEventKind {
    Stdout,
    Stderr,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEvent {
    pub process: String,
    pub kind: ProcessEventKind,
    pub payload: String,
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
        }
    }
}

impl std::error::Error for ProcessManagerError {}

pub struct ProcessSupervisor {
    processes: HashMap<String, Arc<Mutex<Child>>>,
    events_rx: Receiver<ProcessEvent>,
}

impl ProcessSupervisor {
    pub fn spawn(
        repo_root: PathBuf,
        processes: Vec<ProcessSpec>,
    ) -> Result<Self, ProcessManagerError> {
        let (events_tx, events_rx) = mpsc::channel::<ProcessEvent>();
        let mut process_map: HashMap<String, Arc<Mutex<Child>>> =
            HashMap::with_capacity(processes.len());

        for spec in processes {
            let mut child = ProcessCommand::new("sh")
                .arg("-lc")
                .arg(&spec.run)
                .current_dir(&repo_root)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| ProcessManagerError::Spawn {
                    process: spec.name.clone(),
                    command: spec.run.clone(),
                    error,
                })?;

            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| ProcessManagerError::MissingStdio {
                    process: spec.name.clone(),
                })?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| ProcessManagerError::MissingStdio {
                    process: spec.name.clone(),
                })?;

            let child = Arc::new(Mutex::new(child));
            process_map.insert(spec.name.clone(), child.clone());

            {
                let tx = events_tx.clone();
                let process = spec.name.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines().map_while(Result::ok) {
                        let _ = tx.send(ProcessEvent {
                            process: process.clone(),
                            kind: ProcessEventKind::Stdout,
                            payload: line,
                        });
                    }
                });
            }

            {
                let tx = events_tx.clone();
                let process = spec.name.clone();
                thread::spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines().map_while(Result::ok) {
                        let _ = tx.send(ProcessEvent {
                            process: process.clone(),
                            kind: ProcessEventKind::Stderr,
                            payload: line,
                        });
                    }
                });
            }

            {
                let tx = events_tx.clone();
                let process = spec.name.clone();
                thread::spawn(move || {
                    let status = child.lock().expect("child lock").wait();
                    let payload = match status {
                        Ok(status) => format!("exit={}", status.code().unwrap_or(-1)),
                        Err(err) => format!("wait-error={err}"),
                    };
                    let _ = tx.send(ProcessEvent {
                        process,
                        kind: ProcessEventKind::Exit,
                        payload,
                    });
                });
            }
        }

        Ok(Self {
            processes: process_map,
            events_rx,
        })
    }

    pub fn next_event_timeout(&self, timeout: Duration) -> Option<ProcessEvent> {
        self.events_rx.recv_timeout(timeout).ok()
    }

    pub fn send_input(&self, process: &str, input: &str) -> Result<(), ProcessManagerError> {
        let Some(child) = self.processes.get(process) else {
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
        for child in self.processes.values() {
            let _ = child.lock().expect("child lock").kill();
        }
    }
}
