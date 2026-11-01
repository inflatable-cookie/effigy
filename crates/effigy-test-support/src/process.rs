//! Fixture children: one owned process group, stopped when its guard drops.
//!
//! Ownership stays local to the test. A suite-wide sweep in
//! `tests/scripts/suite_guard.py` covers what `Drop` cannot: a killed runner,
//! or a fixture that deliberately leaves its group.
pub use crate::kernel_helpers::wait_kernel_helpers;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output};
use std::time::{Duration, Instant};

/// How long a dropped fixture group has to honour TERM before KILL.
const STOP_GRACE: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(10);
const OUTPUT_DEADLINE: Duration = Duration::from_secs(180);

/// Observe kernel-owned writers before removing a private fixture HOME.
///
/// A fixture's kernel journal is not always at its own root: a fixture that
/// makes its checkout a child directory records the runs and their helpers in
/// that checkout's journal, so waiting only at the root would remove the tree
/// while a recorded supervisor is still writing into it. Every journal this
/// fixture can own is waited for, by the process identities the runs recorded:
/// nothing is guessed, signalled or pattern-matched.
pub fn close_root(owner: &Path) -> io::Result<()> {
    wait_kernel_helpers(owner)?;
    let children = match std::fs::read_dir(owner) {
        Ok(children) => children,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for child in children {
        let child = child?;
        // `file_type` never follows a link: a fixture's own directory is the
        // only thing whose journal is waited for here.
        if !child.file_type()?.is_dir() {
            continue;
        }
        wait_kernel_helpers(&child.path())?;
    }
    Ok(())
}

pub trait FixtureCommandExt {
    fn spawn_owned(&mut self) -> io::Result<OwnedChild>;
    fn output_owned(&mut self) -> io::Result<Output>;
}

impl FixtureCommandExt for Command {
    fn output_owned(&mut self) -> io::Result<Output> {
        self.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn_owned()?
            .wait_with_output()
    }
    fn spawn_owned(&mut self) -> io::Result<OwnedChild> {
        self.process_group(0);
        let mut child = self.spawn()?;
        Ok(OwnedChild {
            stdin: child.stdin.take(),
            stdout: child.stdout.take(),
            stderr: child.stderr.take(),
            group: child.id(),
            reaped: None,
            child,
            deadline: Instant::now() + OUTPUT_DEADLINE,
        })
    }
}

/// Signal the whole group of a fixture launch. Only a group this guard created
/// is ever named, and never after the leader has been reaped.
fn signal_group(group: u32, signal: i32) {
    if let Ok(group) = i32::try_from(group) {
        // SAFETY: the group belongs to a child this guard launched.
        unsafe {
            libc::kill(-group, signal);
        }
    }
}

/// A fixture child that leads its own group; dropping it stops the group.
#[derive(Debug)]
pub struct OwnedChild {
    child: Child,
    group: u32,
    reaped: Option<ExitStatus>,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
    deadline: Instant,
}

impl OwnedChild {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Signal only this retained native leader, matching `std::process::Child`.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    /// Whether the fixture's whole group has ended. Reads the live group, so a
    /// member that outlived its leader is reported.
    pub fn group_closed(&self) -> bool {
        // SAFETY: signal 0 only observes the group this guard created.
        let live = unsafe { libc::kill(-(self.group as i32), 0) } == 0;
        !live && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.reaped.is_some() {
            return Ok(self.reaped);
        }
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.reaped = status;
        }
        Ok(status)
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fixture child deadline",
                ));
            }
            std::thread::sleep(POLL);
        }
    }

    /// Stop the group within its grace, then escalate, and reap the leader.
    pub fn terminate_and_wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.reaped {
            return Ok(status);
        }
        signal_group(self.group, libc::SIGTERM);
        let grace = Instant::now() + STOP_GRACE;
        while Instant::now() < grace {
            if self.try_wait()?.is_some() && self.group_closed() {
                break;
            }
            std::thread::sleep(POLL);
        }
        signal_group(self.group, libc::SIGKILL);
        let status = self.child.wait()?;
        self.reaped = Some(status);
        Ok(status)
    }

    pub fn wait_with_output(mut self) -> io::Result<Output> {
        use std::io::Read;
        fn reader<R: Read + Send + 'static>(
            stream: Option<R>,
        ) -> std::thread::JoinHandle<io::Result<Vec<u8>>> {
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut stream) = stream {
                    stream.read_to_end(&mut bytes)?;
                }
                Ok(bytes)
            })
        }
        drop(self.stdin.take());
        let stdout = reader(self.stdout.take());
        let stderr = reader(self.stderr.take());
        loop {
            if Instant::now() >= self.deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "fixture output deadline",
                ));
            }
            if stdout.is_finished() && stderr.is_finished() {
                if let Some(status) = self.try_wait()? {
                    if !self.group_closed() {
                        return Err(io::Error::other(
                            "fixture child exited with surviving group members",
                        ));
                    }
                    let join = |reader: std::thread::JoinHandle<io::Result<Vec<u8>>>| {
                        reader
                            .join()
                            .map_err(|_| io::Error::other("fixture reader panicked"))?
                    };
                    return Ok(Output {
                        status,
                        stdout: join(stdout)?,
                        stderr: join(stderr)?,
                    });
                }
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // `reaped` is set only after the group was closed, so a settled fixture
        // costs nothing here. Unwinding still stops the group.
        if self.reaped.is_some() {
            return;
        }
        if let Err(error) = self.terminate_and_wait() {
            if std::thread::panicking() {
                eprintln!("fixture process cleanup failed: {error}");
            } else {
                panic!("fixture process cleanup failed: {error}");
            }
        }
    }
}

/// Fake remote exec: the payload's group is the fixture's, so a settled launch
/// is proved to have ended before a completion marker is published.
pub fn run_recorded(command: &mut Command, done: &Path) -> io::Result<()> {
    let mut child = command.spawn_owned()?;
    child.wait()?;
    if !child.group_closed() {
        return Err(io::Error::other(
            "fake remote exited with surviving group members",
        ));
    }
    std::fs::write(done, b"")
}
