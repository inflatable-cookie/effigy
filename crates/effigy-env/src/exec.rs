//! Shell command executor for `exec('...')` value expressions.
//!
//! Runs a command via `sh -c` with a configurable timeout. The child process
//! stdout and stderr are both drained on background threads; if the timeout
//! expires the child is killed and an [`EnvSchemaError::ExecTimeout`] is
//! returned. On success the trimmed stdout content is returned as the resolved
//! value.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::error::EnvSchemaError;

/// Run a shell command and return its trimmed stdout.
///
/// The command is executed with `sh -c` in the given working directory.
/// Background threads drain stdout and stderr while the main thread polls
/// process status until the timeout expires. If the command exceeds the
/// timeout the child is killed.
pub(super) fn run_exec_command(
    command: &str,
    timeout: Duration,
    cwd: &Path,
) -> Result<String, EnvSchemaError> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| EnvSchemaError::ExecSpawn {
            command: command.to_owned(),
            error,
        })?;

    let mut stdout_handle = child.stdout.take().expect("stdout piped");
    let mut stderr_handle = child.stderr.take().expect("stderr piped");

    let stdout_thread = thread::spawn(move || {
        let mut buf = String::new();
        stdout_handle.read_to_string(&mut buf).ok();
        buf
    });
    let stderr_thread = thread::spawn(move || {
        let mut buf = String::new();
        stderr_handle.read_to_string(&mut buf).ok();
        buf
    });

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| EnvSchemaError::ExecSpawn {
                command: command.to_owned(),
                error,
            })?
        {
            let stdout = stdout_thread.join().unwrap_or_default();
            let stderr = stderr_thread.join().unwrap_or_default();

            if !status.success() {
                return Err(EnvSchemaError::ExecFailed {
                    command: command.to_owned(),
                    code: status.code(),
                    stderr: stderr.trim().to_owned(),
                });
            }

            return Ok(stdout.trim().to_owned());
        }

        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(EnvSchemaError::ExecTimeout {
                command: command.to_owned(),
                timeout,
            });
        }

        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::run_exec_command;
    use crate::EnvSchemaError;

    #[test]
    fn stderr_heavy_success_does_not_false_timeout() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let output = run_exec_command(
            "i=0; while [ $i -lt 20000 ]; do echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx >&2; i=$((i+1)); done; echo ok",
            Duration::from_secs(5),
            cwd.path(),
        )
        .expect("stderr-heavy command should still succeed");

        assert_eq!(output, "ok");
    }

    #[test]
    fn stderr_heavy_failure_reports_stderr() {
        let cwd = tempfile::tempdir().expect("tempdir");
        let error = run_exec_command(
            "i=0; while [ $i -lt 2000 ]; do echo fail >&2; i=$((i+1)); done; exit 7",
            Duration::from_secs(5),
            cwd.path(),
        )
        .expect_err("expected failure");

        match error {
            EnvSchemaError::ExecFailed { code, stderr, .. } => {
                assert_eq!(code, Some(7));
                assert!(stderr.contains("fail"), "got: {stderr}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
