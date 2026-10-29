use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
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

/// Children this process is running, plus the spawns that have not published a
/// PID yet.
///
/// A `bun link` still running when its owner is interrupted would recreate the
/// registration the owner is retiring, leaving residue no owner holds. The
/// interrupt cleanup therefore stops these children before it retires anything.
/// Publishing the PID only after `spawn` returns leaves a window in which a
/// started child is invisible, so a spawn reserves its place here *before* it
/// starts and publishes its PID after: cleanup waits for outstanding spawns and
/// refuses new ones, so no started child can escape it.
#[derive(Debug, Default)]
struct OwnedChildren {
    spawning: usize,
    terminating: bool,
    pids: Vec<u32>,
}

static OWNED_CHILDREN: OnceLock<Mutex<OwnedChildren>> = OnceLock::new();

fn owned_children() -> &'static Mutex<OwnedChildren> {
    OWNED_CHILDREN.get_or_init(|| Mutex::new(OwnedChildren::default()))
}

thread_local! {
    /// Spawns made by the interrupt cleanup itself while it retires.
    static CLEANUP_SPAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A reserved spawn. It is published as a tracked child once the OS gives it a
/// PID, and released without one when the spawn fails.
pub(crate) struct SpawnSlot {
    published: bool,
}

impl SpawnSlot {
    fn publish(mut self, pid: u32) -> TrackedChild {
        self.published = true;
        let mut state = owned_children()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.spawning = state.spawning.saturating_sub(1);
        state.pids.push(pid);
        drop(state);
        TrackedChild { pid }
    }
}

impl Drop for SpawnSlot {
    fn drop(&mut self) {
        if self.published {
            return;
        }
        let mut state = owned_children()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.spawning = state.spawning.saturating_sub(1);
    }
}

struct TrackedChild {
    pid: u32,
}

impl Drop for TrackedChild {
    fn drop(&mut self) {
        owned_children()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .pids
            .retain(|pid| *pid != self.pid);
    }
}

/// Reserve the right to start a child process.
///
/// This returns `None` once an interrupt cleanup has begun: starting another
/// `bun` process after that point would let it create a registration the
/// cleanup already passed over.
fn begin_spawn() -> Option<SpawnSlot> {
    let in_cleanup = CLEANUP_SPAWN.with(std::cell::Cell::get);
    let mut state = owned_children()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if state.terminating && !in_cleanup {
        return None;
    }
    state.spawning += 1;
    Some(SpawnSlot { published: false })
}

/// Stop the child and everything it started.
///
/// Each owned child runs in its own process group, so a wrapper's descendant —
/// the `bun` a shim or a script actually runs — dies with it. Killing only the
/// direct child would let a descendant outlive the cleanup, recreate the
/// registration, and keep the ancestor's pipes open so the caller stays blocked
/// while it holds the index lock.
/// A child command that leads its own process group.
///
/// The group is what makes the whole owned tree addressable: terminating it
/// stops the child and everything the child started.
fn owned_command(request: &ProcessRequest) -> Command {
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .current_dir(&request.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs in the forked child before `exec`, where only
    // async-signal-safe work is allowed. `setpgid` is async-signal-safe and
    // makes the child the leader of its own group, so terminating the tree is
    // `kill(-pid, ...)` for every descendant it starts.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                .map_err(std::io::Error::from)
        });
    }
    command
}

#[cfg(unix)]
fn terminate_owned_tree(pid: u32) {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
}

#[cfg(not(unix))]
fn terminate_owned_tree(_pid: u32) {}

/// Run the interrupted owner's cleanup with no child process left running.
///
/// In-flight spawns are waited for (bounded) and any child they publish is
/// stopped, new spawns are refused, and the cleanup's own retiral work is
/// allowed to run — then anything it started and left behind is stopped too.
pub fn interrupt_cleanup<T>(cleanup: impl FnOnce() -> T) -> T {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let (pids, spawning) = {
            let mut state = owned_children()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.terminating = true;
            (state.pids.clone(), state.spawning)
        };
        for pid in pids {
            terminate_owned_tree(pid);
        }
        if spawning == 0 || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let result = CLEANUP_SPAWN.with(|in_cleanup| {
        in_cleanup.set(true);
        let result = cleanup();
        in_cleanup.set(false);
        result
    });
    let leftover = owned_children()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .pids
        .clone();
    for pid in leftover {
        terminate_owned_tree(pid);
    }
    result
}

/// A [`ReadOnlyProcess`] that shares the doctor's remaining monotonic deadline.
///
/// Every child runs in its own process group; when the deadline expires the
/// whole owned tree is terminated and reaped, then a [`DepsError::ProcessTimeout`]
/// is returned so the caller can record a truthful partial report instead of
/// hanging.
///
/// A deadline is mandatory: a new dependency-inspection subprocess call site
/// reaches the doctor budget through this type, so it cannot silently run
/// unbounded. The one explicit opt-out is [`Self::deadline_disabled`], which
/// only the doctor uses when the operator disables its deadline with
/// `EFFIGY_DOCTOR_TIMEOUT_MS=0`. Non-doctor callers name
/// [`StdReadOnlyProcess`] instead, which is unbounded by construction.
#[derive(Debug, Clone, Copy)]
pub struct BoundedReadOnlyProcess {
    deadline: Option<Instant>,
}

impl BoundedReadOnlyProcess {
    /// Run every child under the doctor's remaining deadline.
    pub fn new(deadline: Instant) -> Self {
        Self {
            deadline: Some(deadline),
        }
    }

    /// The explicit unbounded opt-out for a doctor run whose deadline the
    /// operator disabled (`EFFIGY_DOCTOR_TIMEOUT_MS=0`).
    pub fn deadline_disabled() -> Self {
        Self { deadline: None }
    }

    /// The deadline this service runs children under, when it has one.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
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
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setpgid` is
    // async-signal-safe and places the child in a new process group whose id
    // is the child's own pid; the error path uses the allocation-free
    // `io::Error::from` conversion. `terminate_child_tree` signals
    // `kill(-pid, ...)` for that same group when the shared deadline expires.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                .map_err(std::io::Error::from)
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
        // `output()` gives no handle on the running child, and the Bun link
        // owner has to be able to stop one before it retires a registration
        // that child is still creating.
        let Some(slot) = begin_spawn() else {
            return Err(DepsError::ProcessFailed {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                status: None,
                stderr:
                    "an interrupted Effigy run stopped dependency processes; no new one was started"
                        .to_owned(),
            });
        };
        let child = owned_command(request)
            .spawn()
            .map_err(|source| DepsError::ProcessSpawn {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                source,
            })?;
        let tracked = slot.publish(child.id());
        let output = child
            .wait_with_output()
            .map_err(|source| DepsError::ProcessSpawn {
                program: request.program.clone(),
                cwd: request.cwd.clone(),
                source,
            })?;
        drop(tracked);
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
        let process = BoundedReadOnlyProcess::new(
            Instant::now()
                .checked_sub(Duration::from_secs(1))
                .expect("past instant"),
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
    fn explicit_deadline_disabled_opt_out_is_unbounded() {
        let process = BoundedReadOnlyProcess::deadline_disabled();
        assert!(process.deadline().is_none());
        let request = request("true", Vec::new(), std::env::temp_dir());

        let output = process.run(&request).expect("true must succeed");

        assert_eq!(output.status, Some(0));
    }

    #[test]
    fn failing_child_reports_process_failure_not_timeout() {
        let process = BoundedReadOnlyProcess::new(
            Instant::now()
                .checked_add(Duration::from_secs(30))
                .expect("future instant"),
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
        let process = BoundedReadOnlyProcess::new(
            Instant::now()
                .checked_add(Duration::from_millis(300))
                .expect("future instant"),
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

    #[test]
    fn a_spawn_that_has_not_published_its_pid_is_still_stopped_by_cleanup() {
        // The window this covers: `spawn` returned, so the child is running,
        // but its PID has not reached the registry that cleanup reads. The
        // child is started the way the owned path starts one, in its own
        // process group, so cleanup can stop its whole tree.
        let mut child = owned_command(&ProcessRequest {
            program: "sleep".to_owned(),
            args: vec!["10".to_owned()],
            cwd: std::env::temp_dir(),
        })
        .spawn()
        .unwrap();
        let slot = begin_spawn().expect("a spawn is allowed before cleanup");
        let cleanup = thread::spawn(|| crate::process::interrupt_cleanup(|| ()));
        thread::sleep(Duration::from_millis(100));
        assert!(
            !owned_children()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .pids
                .contains(&child.id()),
            "the child must still be unpublished when cleanup starts"
        );
        let tracked = slot.publish(child.id());
        let status = child.wait().expect("cleanup stops the published child");
        drop(tracked);
        cleanup.join().unwrap();

        assert!(
            !status.success(),
            "a child published after cleanup began escaped it"
        );
    }

    #[test]
    fn no_child_can_start_after_interrupt_cleanup_begins() {
        let slot = begin_spawn().expect("a spawn is allowed before cleanup");
        drop(slot);
        crate::process::interrupt_cleanup(|| ());
        assert!(
            begin_spawn().is_none(),
            "a spawn started after interrupt cleanup would escape it"
        );
    }
}
