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

#[cfg(target_os = "macos")]
const DEMO_BROWSER_TERMINAL_COLS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_COLS";
#[cfg(target_os = "macos")]
const DEMO_BROWSER_TERMINAL_ROWS_ENV: &str = "EFFIGY_BROWSER_TERMINAL_ROWS";

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
        .arg("-c")
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
    for (key, value) in &spec.env {
        process.env(key, value);
    }
    process
}

fn spawn_with_pty_wrapper(spec: &ProcessSpec) -> ProcessCommand {
    #[cfg(target_os = "macos")]
    {
        let terminal_size = browser_terminal_size_override();
        let wrapped_run = wrap_pty_shell_command(&spec.run, terminal_size);
        let mut process = ProcessCommand::new("script");
        process
            .arg("-q")
            .arg("/dev/null")
            .arg("sh")
            .arg("-c")
            .arg(wrapped_run)
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
        if let Some((cols, rows)) = terminal_size {
            process
                .env("COLUMNS", cols.to_string())
                .env("LINES", rows.to_string());
        }
        for (key, value) in &spec.env {
            process.env(key, value);
        }
        process
    }

    #[cfg(not(target_os = "macos"))]
    {
        spawn_plain_shell(spec)
    }
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

#[cfg(target_os = "macos")]
fn browser_terminal_size_override() -> Option<(u16, u16)> {
    let cols = std::env::var(DEMO_BROWSER_TERMINAL_COLS_ENV)
        .ok()?
        .parse::<u16>()
        .ok()?;
    let rows = std::env::var(DEMO_BROWSER_TERMINAL_ROWS_ENV)
        .ok()?
        .parse::<u16>()
        .ok()?;
    if cols == 0 || rows == 0 {
        return None;
    }
    Some((cols, rows))
}

#[cfg(target_os = "macos")]
fn wrap_pty_shell_command(run_command: &str, terminal_size: Option<(u16, u16)>) -> String {
    let Some((cols, rows)) = terminal_size else {
        return run_command.to_owned();
    };
    format!("stty cols {cols} rows {rows} >/dev/null 2>&1; {run_command}")
}
