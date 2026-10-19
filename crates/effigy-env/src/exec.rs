//! Shell command executor for `exec('...')` value expressions.
//!
//! A caller that owns a run passes its own [`ExecRunner`], so each command is
//! an effect of that run (contract 052 K3). Without one, [`run_exec_command`]
//! runs `sh -c` under the configured timeout with bounded capture: stdout
//! beyond [`STDOUT_MAX_BYTES`] is refused and only the last
//! [`STDERR_TAIL_BYTES`] of stderr are kept. On timeout the child is killed
//! and the readers are abandoned rather than waited on, so a descendant that
//! holds a pipe open cannot extend the wait.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::error::EnvSchemaError;

/// The most stdout an `exec()` value may produce.
pub const STDOUT_MAX_BYTES: usize = 1 << 20;
/// The stderr tail kept for a failure message.
pub const STDERR_TAIL_BYTES: usize = 64 * 1024;

/// Runs one `exec()` command and returns its trimmed stdout.
pub trait ExecRunner {
    fn run(&self, command: &str, timeout: Duration, cwd: &Path) -> Result<String, EnvSchemaError>;
}

/// Run a shell command and return its trimmed stdout, with bounded capture.
pub(super) fn run_exec_command(
    command: &str,
    timeout: Duration,
    cwd: &Path,
) -> Result<String, EnvSchemaError> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| EnvSchemaError::ExecSpawn {
            command: command.to_owned(),
            error,
        })?;

    let (stdout_tx, stdout_rx) = mpsc::channel();
    let (stderr_tx, stderr_rx) = mpsc::channel();
    let stdout_handle = child.stdout.take().expect("stdout piped");
    let stderr_handle = child.stderr.take().expect("stderr piped");
    thread::spawn(move || {
        let _ = stdout_tx.send(read_bounded(stdout_handle, STDOUT_MAX_BYTES, false));
    });
    thread::spawn(move || {
        let _ = stderr_tx.send(read_bounded(stderr_handle, STDERR_TAIL_BYTES, true));
    });

    let deadline = Instant::now() + timeout;
    let timed_out = || EnvSchemaError::ExecTimeout {
        command: command.to_owned(),
        timeout,
    };
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| EnvSchemaError::ExecSpawn {
                command: command.to_owned(),
                error,
            })?
        {
            // A descendant holding a pipe open must not outlast the budget.
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (stdout, overflow) = stdout_rx.recv_timeout(remaining).map_err(|_| timed_out())?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            let (stderr, _) = stderr_rx.recv_timeout(remaining).unwrap_or_default();
            if overflow {
                return Err(EnvSchemaError::ExecFailed {
                    command: command.to_owned(),
                    code: status.code(),
                    stderr: format!("stdout exceeded {STDOUT_MAX_BYTES} bytes"),
                });
            }
            if !status.success() {
                return Err(EnvSchemaError::ExecFailed {
                    command: command.to_owned(),
                    code: status.code(),
                    stderr: String::from_utf8_lossy(&stderr).trim().to_owned(),
                });
            }
            return Ok(String::from_utf8_lossy(&stdout).trim().to_owned());
        }

        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            return Err(timed_out());
        }

        thread::sleep(Duration::from_millis(10));
    }
}

/// Reads to EOF keeping at most `limit` bytes: the head (with an overflow
/// flag) or, for `tail`, the newest bytes.
fn read_bounded(mut reader: impl Read, limit: usize, tail: bool) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut overflow = false;
    let mut buffer = [0u8; 8192];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(count) => count,
        };
        kept.extend_from_slice(&buffer[..count]);
        if kept.len() > limit {
            overflow = true;
            if tail {
                let excess = kept.len() - limit;
                kept.drain(..excess);
            } else {
                kept.truncate(limit);
            }
        }
    }
    (kept, overflow)
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
