use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

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
            let child = spawn_process_instance(&spec, &events_tx, true)?;
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
            let _ = child.lock().expect("child lock").kill();
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
        terminate_child_graceful(&child, Duration::from_millis(800));
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
                terminate_child_graceful(child, Duration::from_millis(800));
            }
        }
        let mut restart_spec = spec;
        restart_spec.start_after_ms = 0;
        let replacement = spawn_process_instance(&restart_spec, &self.events_tx, false)?;
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
            #[cfg(unix)]
            {
                let _ = signal_process_group(&mut child, Signal::SIGTERM);
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
            }
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
            #[cfg(unix)]
            {
                let _ = signal_process_group(&mut child, Signal::SIGKILL);
                forced += 1;
            }
            #[cfg(not(unix))]
            {
                let _ = child.kill();
                forced += 1;
            }
        }
        on_progress(ShutdownProgress::Complete {
            total: children.len(),
            forced,
        });
    }

    pub fn exit_diagnostics(&self) -> Vec<(String, String)> {
        let process_map = self.processes.lock().expect("process map lock");
        let mut diagnostics = self
            .specs
            .iter()
            .map(|(name, _)| {
                let diagnostic = if let Some(child) = process_map.get(name) {
                    match child.lock().expect("child lock").try_wait() {
                        Ok(Some(status)) => format_exit_diagnostic(status),
                        Ok(None) => "running".to_owned(),
                        Err(err) => format!("wait-error={err}"),
                    }
                } else {
                    "not-tracked".to_owned()
                };
                (name.clone(), diagnostic)
            })
            .collect::<Vec<(String, String)>>();
        diagnostics.sort_by(|a, b| a.0.cmp(&b.0));
        diagnostics
    }
}

fn format_exit_diagnostic(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        if let Some(code) = status.code() {
            return format!("exit={code}");
        }
        if let Some(signal) = status.signal() {
            return format!("signal={signal}");
        }
        "exit=unknown".to_owned()
    }
    #[cfg(not(unix))]
    {
        format!("exit={}", status.code().unwrap_or(-1))
    }
}

fn spawn_process_instance(
    spec: &ProcessSpec,
    events_tx: &Sender<ProcessEvent>,
    honor_start_delay: bool,
) -> Result<Arc<Mutex<Child>>, ProcessManagerError> {
    if honor_start_delay && spec.start_after_ms > 0 {
        thread::sleep(Duration::from_millis(spec.start_after_ms));
    }
    let mut process = if spec.pty {
        spawn_with_pty_wrapper(spec)
    } else {
        spawn_plain_shell(spec)
    };
    let mut child = process
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
    attach_child_stream_threads(spec.name.clone(), child.clone(), stdout, stderr, events_tx);
    Ok(child)
}

fn attach_child_stream_threads(
    process_name: String,
    child: Arc<Mutex<Child>>,
    stdout: impl std::io::Read + Send + 'static,
    stderr: impl std::io::Read + Send + 'static,
    events_tx: &Sender<ProcessEvent>,
) {
    spawn_stream_thread(
        process_name.clone(),
        stdout,
        ProcessEventKind::Stdout,
        ProcessEventKind::StdoutChunk,
        events_tx.clone(),
    );
    spawn_stream_thread(
        process_name.clone(),
        stderr,
        ProcessEventKind::Stderr,
        ProcessEventKind::StderrChunk,
        events_tx.clone(),
    );

    {
        let tx = events_tx.clone();
        thread::spawn(move || loop {
            let status = child.lock().expect("child lock").try_wait();
            match status {
                Ok(Some(status)) => {
                    let payload = format_exit_diagnostic(status);
                    let _ = tx.send(ProcessEvent {
                        process: process_name.clone(),
                        kind: ProcessEventKind::Exit,
                        payload,
                        chunk: None,
                    });
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(40)),
                Err(err) => {
                    let _ = tx.send(ProcessEvent {
                        process: process_name.clone(),
                        kind: ProcessEventKind::Exit,
                        payload: format!("wait-error={err}"),
                        chunk: None,
                    });
                    break;
                }
            }
        });
    }
}

fn spawn_stream_thread(
    process: String,
    mut reader: impl Read + Send + 'static,
    line_kind: ProcessEventKind,
    chunk_kind: ProcessEventKind,
    tx: Sender<ProcessEvent>,
) {
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line_buffer = Vec::<u8>::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(read) => {
                    let chunk = buf[..read].to_vec();
                    let _ = tx.send(ProcessEvent {
                        process: process.clone(),
                        kind: chunk_kind.clone(),
                        payload: String::from_utf8_lossy(&chunk).into_owned(),
                        chunk: Some(chunk.clone()),
                    });
                    line_buffer.extend_from_slice(&chunk);
                    emit_complete_lines(&tx, &process, &line_kind, &mut line_buffer);
                }
                Err(_) => break,
            }
        }
        if !line_buffer.is_empty() {
            let line = decode_line(&line_buffer);
            let _ = tx.send(ProcessEvent {
                process,
                kind: line_kind,
                payload: line,
                chunk: None,
            });
        }
    });
}

fn emit_complete_lines(
    tx: &Sender<ProcessEvent>,
    process: &str,
    line_kind: &ProcessEventKind,
    line_buffer: &mut Vec<u8>,
) {
    loop {
        let Some(index) = line_buffer.iter().position(|byte| *byte == b'\n') else {
            break;
        };
        let line = line_buffer.drain(..=index).collect::<Vec<u8>>();
        let text = decode_line(&line);
        let _ = tx.send(ProcessEvent {
            process: process.to_owned(),
            kind: line_kind.clone(),
            payload: text,
            chunk: None,
        });
    }
}

fn decode_line(line: &[u8]) -> String {
    let mut slice = line;
    if slice.ends_with(b"\n") {
        slice = &slice[..slice.len() - 1];
    }
    if slice.ends_with(b"\r") {
        slice = &slice[..slice.len() - 1];
    }
    String::from_utf8_lossy(slice).into_owned()
}

fn terminate_child_graceful(child: &Arc<Mutex<Child>>, timeout: Duration) {
    {
        let mut child = child.lock().expect("child lock");
        #[cfg(unix)]
        {
            let _ = signal_process_group(&mut child, Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        {
            let _ = child.kill();
        }
    }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let exited = child
            .lock()
            .expect("child lock")
            .try_wait()
            .ok()
            .flatten()
            .is_some();
        if exited {
            return;
        }
        thread::sleep(Duration::from_millis(30));
    }
    let mut child = child.lock().expect("child lock");
    #[cfg(unix)]
    {
        let _ = signal_process_group(&mut child, Signal::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

fn spawn_plain_shell(spec: &ProcessSpec) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-lc")
        .arg(&spec.run)
        .current_dir(&spec.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        process.pre_exec(|| {
            setpgid(Pid::from_raw(0), Pid::from_raw(0))
                .map_err(|error| std::io::Error::new(ErrorKind::Other, error.to_string()))
        });
    }
    with_local_node_bin_path(&mut process, &spec.cwd);
    process
}

fn spawn_with_pty_wrapper(spec: &ProcessSpec) -> ProcessCommand {
    #[cfg(target_os = "macos")]
    {
        let mut process = ProcessCommand::new("script");
        process
            .arg("-q")
            .arg("/dev/null")
            .arg("sh")
            .arg("-lc")
            .arg(&spec.run)
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        unsafe {
            process.pre_exec(|| {
                setpgid(Pid::from_raw(0), Pid::from_raw(0))
                    .map_err(|error| std::io::Error::new(ErrorKind::Other, error.to_string()))
            });
        }
        with_local_node_bin_path(&mut process, &spec.cwd);
        return process;
    }

    #[allow(unreachable_code)]
    spawn_plain_shell(spec)
}

fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return;
    }
    let local_rendered = local_bin.display().to_string();
    let merged = match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    };
    process.env("PATH", merged);
}

#[cfg(unix)]
fn signal_process_group(child: &mut Child, signal: Signal) -> Result<(), nix::Error> {
    let pid = child.id() as i32;
    if pid > 0 {
        kill(Pid::from_raw(-pid), signal)
    } else {
        Ok(())
    }
}
