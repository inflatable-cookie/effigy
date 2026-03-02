#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

use super::super::{ProcessEvent, ProcessManagerError, ProcessSpec};
use super::monitor::attach_child_stream_threads;

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
