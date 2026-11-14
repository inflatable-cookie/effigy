//! Fixture children: one owned process group, stopped when its guard drops.
//!
//! Ownership stays local to the test. A suite-wide sweep in
//! `tests/scripts/suite_guard.py` covers what `Drop` cannot: a killed runner,
//! or a fixture that deliberately leaves its group.
pub use crate::kernel_helpers::{wait_kernel_helpers, wait_kernel_helpers_within};
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output};
use std::time::{Duration, Instant};

/// How long a dropped fixture group has to honour TERM before KILL.
const STOP_GRACE: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(10);
const OUTPUT_DEADLINE: Duration = Duration::from_secs(180);

/// Shell tail for a fixture that must stop when its runner's pipe closes.
/// The runner owns the pipe writer; its death closes it without polling.
pub fn parent_exit_guard() -> &'static str {
    "cat >/dev/null"
}

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
        group_members(self.group).is_ok_and(|members| members.is_empty())
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
        signal_group(self.group, libc::SIGTERM);
        let grace = Instant::now() + STOP_GRACE;
        while Instant::now() < grace {
            if self.try_wait()?.is_some() && self.group_closed() {
                break;
            }
            std::thread::sleep(POLL);
        }
        signal_group(self.group, libc::SIGKILL);
        let status = match self.reaped {
            Some(status) => status,
            None => self.child.wait()?,
        };
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
                        wait_group_members(self.group, self.deadline)?;
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

/// Current live members of an owned process group. A zombie has exited and
/// cannot retain fixture resources; `kill(-pgid, 0)` incorrectly counts it
/// until its parent reaps it.
fn group_members(group: u32) -> io::Result<Vec<u32>> {
    let output = Command::new("ps")
        .args(["-Ao", "pid=,pgid=,stat="])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("process table listing command failed"));
    }
    let mut members = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let pid = fields
            .next()
            .and_then(|field| field.parse::<u32>().ok())
            .ok_or_else(|| io::Error::other("process table listing contains a malformed row"))?;
        let process_group = fields
            .next()
            .and_then(|field| field.parse::<u32>().ok())
            .ok_or_else(|| io::Error::other("process table listing contains a malformed row"))?;
        let state = fields
            .next()
            .ok_or_else(|| io::Error::other("process table listing contains a malformed row"))?;
        if pid > 0 && process_group == group && !state.starts_with('Z') {
            members.push(pid);
        }
    }
    Ok(members)
}

/// Wait for a live owned-group member's exit notification, then re-read the
/// group. The process exit event avoids sampling the narrow interval between
/// a child closing its pipes and the kernel recording its exit.
fn wait_group_members(group: u32, deadline: Instant) -> io::Result<()> {
    loop {
        let members = group_members(group)?;
        if members.is_empty() {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "fixture child exited with surviving group members",
            ));
        }
        wait_for_process_exit(&members, remaining)?;
    }
}

#[cfg(target_os = "macos")]
fn wait_for_process_exit(pids: &[u32], timeout: Duration) -> io::Result<()> {
    use std::os::fd::RawFd;

    // SAFETY: kqueue has no pointer arguments or caller preconditions.
    let queue: RawFd = unsafe { libc::kqueue() };
    if queue < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut watched = 0;
    for pid in pids {
        let change = libc::kevent {
            ident: usize::try_from(*pid).unwrap_or(usize::MAX),
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        // SAFETY: queue is open; `change` is initialized and readable. The
        // output pointer is null with a zero event count, and the timeout is
        // null for this nonblocking registration operation.
        let result =
            unsafe { libc::kevent(queue, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
        if result == 0 {
            watched += 1;
        } else {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                // The registration may or may not have taken effect. Closing
                // this queue and resnapshotting makes the next pass exact.
                // SAFETY: queue is the descriptor returned by kqueue above.
                unsafe { libc::close(queue) };
                return Ok(());
            }
            if error.raw_os_error() != Some(libc::ESRCH) {
                // SAFETY: queue is the descriptor returned by kqueue above.
                unsafe { libc::close(queue) };
                return Err(error);
            }
        }
    }
    if watched == 0 {
        // SAFETY: queue is the descriptor returned by kqueue above.
        unsafe { libc::close(queue) };
        return Ok(());
    }
    let timeout = libc::timespec {
        tv_sec: timeout.as_secs().try_into().unwrap_or(libc::time_t::MAX),
        tv_nsec: i64::from(timeout.subsec_nanos()),
    };
    let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();
    // SAFETY: queue is open; the output pointer addresses one writable event,
    // and timeout points to an initialized timespec for this wait.
    let result =
        unsafe { libc::kevent(queue, std::ptr::null(), 0, event.as_mut_ptr(), 1, &timeout) };
    let error = if result < 0 {
        let error = io::Error::last_os_error();
        (error.kind() != io::ErrorKind::Interrupted).then_some(error)
    } else if result == 0 {
        Some(io::Error::new(
            io::ErrorKind::TimedOut,
            "fixture process exit event",
        ))
    } else {
        None
    };
    // SAFETY: queue is the descriptor returned by kqueue above.
    unsafe { libc::close(queue) };
    error.map_or(Ok(()), Err)
}

#[cfg(target_os = "linux")]
fn wait_for_process_exit(pids: &[u32], timeout: Duration) -> io::Result<()> {
    let mut descriptors = Vec::new();
    for pid in pids {
        // SAFETY: pidfd_open takes only the positive PID from the process
        // table and flags by value; neither argument points into caller memory.
        let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, *pid as libc::pid_t, 0) };
        if descriptor >= 0 {
            descriptors.push(libc::pollfd {
                fd: descriptor as libc::c_int,
                events: libc::POLLIN,
                revents: 0,
            });
        } else if io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            let error = io::Error::last_os_error();
            for descriptor in descriptors {
                // SAFETY: each descriptor is a pidfd opened above.
                unsafe { libc::close(descriptor.fd) };
            }
            return Err(error);
        }
    }
    if descriptors.is_empty() {
        return Ok(());
    }
    let millis = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: poll receives the initialized descriptor slice for its length.
    let result = unsafe {
        libc::poll(
            descriptors.as_mut_ptr(),
            descriptors.len() as libc::nfds_t,
            millis,
        )
    };
    let error = if result < 0 {
        let error = io::Error::last_os_error();
        (error.kind() != io::ErrorKind::Interrupted).then_some(error)
    } else if result == 0 {
        Some(io::Error::new(
            io::ErrorKind::TimedOut,
            "fixture process exit event",
        ))
    } else {
        None
    };
    for descriptor in descriptors {
        // SAFETY: each descriptor is a pidfd opened above.
        unsafe { libc::close(descriptor.fd) };
    }
    error.map_or(Ok(()), Err)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn wait_for_process_exit(_pids: &[u32], _timeout: Duration) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "fixture process exit events are unsupported on this platform",
    ))
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        // A reaped leader can still have live group members. Check the group
        // before treating this launch as settled; unwinding stops survivors.
        if self.reaped.is_some() && self.group_closed() {
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
