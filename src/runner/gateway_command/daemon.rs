use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use effigy_gateway::server::{self, GatewayConfig};

use crate::runner::error::RunnerError;

use super::gateway_dir;

pub(super) fn spawn_gateway_daemon(config: &GatewayConfig) -> Result<(), RunnerError> {
    std::fs::create_dir_all(gateway_dir()?).map_err(RunnerError::Cwd)?;
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let stdout_log =
        std::fs::File::create(gateway_stdout_log_path(config)).map_err(RunnerError::Cwd)?;
    let stderr_log =
        std::fs::File::create(gateway_stderr_log_path(config)).map_err(RunnerError::Cwd)?;
    let mut command = ProcessCommand::new(&effigy_bin);
    command
        .arg("__gateway-run")
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    unsafe {
        command.pre_exec(|| {
            #[cfg(unix)]
            {
                if nix::libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: format!("{} __gateway-run", effigy_bin.display()),
            error,
        })?;

    for _ in 0..10 {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| RunnerError::TaskCommandLaunch {
                command: "__gateway-run".to_owned(),
                error,
            })?
        {
            let stderr =
                std::fs::read_to_string(gateway_stderr_log_path(config)).unwrap_or_default();
            let stdout =
                std::fs::read_to_string(gateway_stdout_log_path(config)).unwrap_or_default();
            let detail = if !stderr.is_empty() {
                normalize_gateway_daemon_output(stderr.trim())
            } else if !stdout.is_empty() {
                normalize_gateway_daemon_output(stdout.trim())
            } else {
                "gateway daemon exited without diagnostic output".to_owned()
            };
            return Err(RunnerError::task_invocation(format!(
                "gateway daemon exited immediately with status {status}: {detail}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }

    Ok(())
}

pub(super) fn wait_for_pid_file(config: &GatewayConfig) -> Result<(), RunnerError> {
    for _ in 0..20 {
        if config.pid_file_path.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(RunnerError::task_invocation(format!(
        "gateway did not create pid file at {}",
        config.pid_file_path.display()
    )))
}

pub(super) fn stop_gateway_process(pid: u32) -> Result<(), RunnerError> {
    terminate_gateway_process(pid)?;
    for _ in 0..40 {
        if !server::process_is_running(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGKILL)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        for _ in 0..20 {
            if !server::process_is_running(pid) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    Err(RunnerError::task_invocation(format!(
        "gateway process {pid} did not stop after SIGTERM/SIGKILL"
    )))
}

fn terminate_gateway_process(pid: u32) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(RunnerError::task_invocation(
            "`effigy gateway down` is not implemented on this host platform yet",
        ))
    }
}

pub(super) fn normalize_gateway_daemon_output(text: &str) -> String {
    let lines = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "[error] Task failed" {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<_>>();
    let base = if lines.is_empty() {
        text.trim().to_owned()
    } else {
        lines.join(" ")
    };
    if (base.contains("127.0.0.1:80") || base.contains("127.0.0.1:443"))
        && base.contains("Permission denied")
    {
        format!(
            "{base}. binding the HTTP/HTTPS gateway to privileged ports requires elevated privileges on this machine"
        )
    } else {
        base
    }
}

fn gateway_stdout_log_path(config: &GatewayConfig) -> PathBuf {
    config
        .pid_file_path
        .parent()
        .unwrap_or(config.pid_file_path.as_path())
        .join("gateway.stdout.log")
}

fn gateway_stderr_log_path(config: &GatewayConfig) -> PathBuf {
    config
        .pid_file_path
        .parent()
        .unwrap_or(config.pid_file_path.as_path())
        .join("gateway.stderr.log")
}
