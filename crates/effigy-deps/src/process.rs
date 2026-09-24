use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::DepsError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait ReadOnlyProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdReadOnlyProcess;

/// A [`ReadOnlyProcess`] that shares the doctor's remaining monotonic deadline.
///
/// Every child runs in its own process group; when the deadline expires the
/// whole owned tree is terminated and reaped, then a [`DepsError::ProcessTimeout`]
/// is returned so the caller can record a truthful partial report instead of
/// hanging. A `None` deadline preserves the unbounded [`StdReadOnlyProcess`]
/// behavior for non-doctor callers.
#[derive(Debug, Clone, Copy)]
pub struct BoundedReadOnlyProcess {
    deadline: Option<Instant>,
}

impl BoundedReadOnlyProcess {
    pub fn with_deadline(deadline: Option<Instant>) -> Self {
        Self { deadline }
    }
}

impl ReadOnlyProcess for BoundedReadOnlyProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        let Some(deadline) = self.deadline else {
            return StdReadOnlyProcess.run(request);
        };
        run_process_bounded(request, deadline)
    }
}

fn run_process_bounded(
    request: &ProcessRequest,
    deadline: Instant,
) -> Result<ProcessOutput, DepsError> {
    let started = Instant::now();
    if started >= deadline {
        return Err(DepsError::ProcessTimeout {
            program: request.program.clone(),
            cwd: request.cwd.clone(),
            budget: Duration::ZERO,
        });
    }
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                .map_err(|error| std::io::Error::other(error.to_string()))
        });
    }
    let mut child = command.spawn().map_err(|source| DepsError::ProcessSpawn {
        program: request.program.clone(),
        cwd: request.cwd.clone(),
        source,
    })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DepsError::ProcessFailed {
            program: request.program.clone(),
            cwd: request.cwd.clone(),
            status: None,
            stderr: "dependency process stdout was unavailable".to_owned(),
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DepsError::ProcessFailed {
            program: request.program.clone(),
            cwd: request.cwd.clone(),
            status: None,
            stderr: "dependency process stderr was unavailable".to_owned(),
        })?;
    let stdout_reader = thread::spawn(move || read_child_stream(stdout));
    let stderr_reader = thread::spawn(move || read_child_stream(stderr));
    loop {
        if let Some(status) = child.try_wait().map_err(|error| DepsError::ProcessFailed {
            program: request.program.clone(),
            cwd: request.cwd.clone(),
            status: None,
            stderr: format!("failed to poll dependency process: {error}"),
        })? {
            let stdout = stdout_reader.join().unwrap_or_default();
            let stderr = stderr_reader.join().unwrap_or_default();
            let result = ProcessOutput {
                status: status.code(),
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
            };
            if status.success() {
                return Ok(result);
            }
            return Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: result.status,
                stderr: result.stderr,
            });
        }

        if Instant::now() >= deadline {
            terminate_child_tree(&mut child);
            let grace_deadline = Instant::now() + Duration::from_millis(500);
            loop {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                if Instant::now() >= grace_deadline {
                    terminate_child_tree_forced(&mut child);
                    let _ = child.wait();
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(DepsError::ProcessTimeout {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                budget: started.elapsed(),
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_child_stream(mut stream: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = stream.read_to_end(&mut bytes);
    bytes
}

/// Reports whether a dependency error is a shared-deadline expiry that the
/// caller must propagate as budget exhaustion rather than diagnosis.
pub fn is_process_timeout(error: &DepsError) -> bool {
    matches!(error, DepsError::ProcessTimeout { .. })
}

#[cfg(unix)]
fn terminate_child_tree(child: &mut std::process::Child) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    let pid = child.id() as i32;
    if pid <= 0 {
        return;
    }
    let _ = kill(Pid::from_raw(-pid), Signal::SIGTERM);
    let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
}

#[cfg(unix)]
fn terminate_child_tree_forced(child: &mut std::process::Child) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    let pid = child.id() as i32;
    if pid <= 0 {
        return;
    }
    let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
    let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
}

#[cfg(not(unix))]
fn terminate_child_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(not(unix))]
fn terminate_child_tree_forced(child: &mut std::process::Child) {
    let _ = child.kill();
}

impl ReadOnlyProcess for StdReadOnlyProcess {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, DepsError> {
        let output = Command::new(&request.program)
            .args(&request.args)
            .current_dir(&request.cwd)
            .output()
            .map_err(|source| DepsError::ProcessSpawn {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                source,
            })?;
        let result = ProcessOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };
        if output.status.success() {
            Ok(result)
        } else {
            Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: result.status,
                stderr: result.stderr,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(program: &str, args: Vec<&str>, cwd: PathBuf) -> ProcessRequest {
        ProcessRequest {
            program: program.to_owned(),
            args: args.into_iter().map(str::to_owned).collect(),
            cwd,
        }
    }

    #[test]
    fn expired_deadline_times_out_without_spawning() {
        let process = BoundedReadOnlyProcess::with_deadline(
            Instant::now().checked_sub(Duration::from_secs(1)),
        );
        let request = request(
            "definitely-not-a-real-program",
            Vec::new(),
            std::env::temp_dir(),
        );

        let error = process
            .run(&request)
            .expect_err("expired deadline must not spawn");

        assert!(
            matches!(error, DepsError::ProcessTimeout { .. }),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn unbounded_deadline_preserves_std_behavior() {
        let process = BoundedReadOnlyProcess::with_deadline(None);
        let request = request("true", Vec::new(), std::env::temp_dir());

        let output = process.run(&request).expect("true must succeed");

        assert_eq!(output.status, Some(0));
    }

    #[test]
    fn failing_child_reports_process_failure_not_timeout() {
        let process = BoundedReadOnlyProcess::with_deadline(
            Instant::now().checked_add(Duration::from_secs(30)),
        );
        let request = request("false", Vec::new(), std::env::temp_dir());

        let error = process.run(&request).expect_err("false must fail");

        assert!(
            matches!(error, DepsError::ProcessFailed { .. }),
            "unexpected error: {error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn expired_budget_terminates_the_owned_process_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("survivor");
        // The descendant writes the marker only if it survives three seconds
        // past the bounded call, while the foreground sleep keeps the direct
        // child alive for thirty: returning proves the direct child was
        // reaped, and the absent marker proves the tree died with it.
        let script = format!("(sleep 3; printf leaked > {}) & sleep 30", marker.display());
        let process = BoundedReadOnlyProcess::with_deadline(
            Instant::now().checked_add(Duration::from_millis(300)),
        );
        let request = request("sh", vec!["-lc", &script], dir.path().to_path_buf());

        let started = Instant::now();
        let error = process
            .run(&request)
            .expect_err("blocked child must time out");
        assert!(
            matches!(error, DepsError::ProcessTimeout { .. }),
            "unexpected error: {error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "bounded child held the caller past its budget"
        );
        std::thread::sleep(Duration::from_secs(4));
        assert!(
            !marker.exists(),
            "timed-out dependency child tree survived the deadline"
        );
    }
}
