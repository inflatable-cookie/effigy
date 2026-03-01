use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

use super::streams::spawn_stream_thread;
use super::{ProcessEvent, ProcessEventKind, ProcessManagerError, ProcessSpec};

pub(super) fn spawn_process_instance(
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
    stdout: impl Read + Send + 'static,
    stderr: impl Read + Send + 'static,
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
                    let payload = super::format_exit_diagnostic(status);
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

pub(super) fn terminate_child_graceful(child: &Arc<Mutex<Child>>, timeout: Duration) {
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
                .map_err(|error| std::io::Error::other(error.to_string()))
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
                    .map_err(|error| std::io::Error::other(error.to_string()))
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
