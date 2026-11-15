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
/// How long a launch's kernel helpers get to exit *after* its leader is gone.
/// The output deadline bounds the child's own life and is spent by the work the
/// fixture asked for, so a slow-but-correct launch must not also spend the
/// window its helpers are proved settled in (effigy#998).
const HELPER_SETTLE: Duration = Duration::from_secs(30);

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
    /// Spawn while preserving a process group or session the caller set up.
    /// That group must be the child's PID, so this guard can own it precisely.
    fn spawn_owned_preserving_group(&mut self) -> io::Result<OwnedChild>;
    fn output_owned(&mut self) -> io::Result<Output>;
    fn status_owned(&mut self) -> io::Result<ExitStatus>;
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
        spawn_owned_inner(self, true)
    }
    fn spawn_owned_preserving_group(&mut self) -> io::Result<OwnedChild> {
        spawn_owned_inner(self, false)
    }
    fn status_owned(&mut self) -> io::Result<ExitStatus> {
        self.stdin(std::process::Stdio::null())
            .spawn_owned()?
            .wait()
    }
}

fn spawn_owned_inner(command: &mut Command, establish_group: bool) -> io::Result<OwnedChild> {
    let watches_helpers = is_effigy_executable(command.get_program());
    if watches_helpers
        && !command
            .get_envs()
            .any(|(key, _)| key == std::ffi::OsStr::new("NEXTEST_TEST_NAME"))
    {
        let test_name = std::env::var_os("NEXTEST_TEST_NAME").or_else(|| {
            std::thread::current()
                .name()
                .filter(|name| !name.is_empty())
                .map(std::ffi::OsString::from)
        });
        if let Some(test_name) = test_name {
            let marker = test_name.to_string_lossy();
            let arg0 = format!(
                "{} --effigy-suite-test={marker}",
                command.get_program().to_string_lossy()
            );
            command.env("NEXTEST_TEST_NAME", test_name).arg0(arg0);
        }
    }
    if establish_group {
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let pid = child.id();
    Ok(OwnedChild {
        stdin: child.stdin.take(),
        stdout: child.stdout.take(),
        stderr: child.stderr.take(),
        group: pid,
        reaped: None,
        child,
        deadline: Instant::now() + OUTPUT_DEADLINE,
        helpers: Vec::new(),
        watches_helpers,
    })
}

fn is_effigy_executable(program: &std::ffi::OsStr) -> bool {
    Path::new(program)
        .file_name()
        .is_some_and(|name| name == "effigy")
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
    helpers: Vec<effigy_kernel::ident::ProcessIdentity>,
    watches_helpers: bool,
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
            self.capture_kernel_helpers()?;
            if let Some(status) = self.try_wait()? {
                self.wait_for_helpers()?;
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
        self.capture_kernel_helpers()?;
        signal_group(self.group, libc::SIGTERM);
        let grace = Instant::now() + STOP_GRACE;
        while Instant::now() < grace {
            self.capture_kernel_helpers()?;
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
        self.wait_for_helpers()?;
        Ok(status)
    }

    pub fn wait_with_output(self) -> io::Result<Output> {
        self.wait_with_output_inner()
    }

    fn capture_kernel_helpers(&mut self) -> io::Result<()> {
        if !self.watches_helpers {
            return Ok(());
        }
        let owner_marker = format!("--effigy-suite-parent={}", self.child.id());
        let output = Command::new("ps")
            .args(["-axo", "pid=,ppid=,args="])
            .output()?;
        if !output.status.success() {
            return Err(io::Error::other("process table listing command failed"));
        }
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = line.split_whitespace();
            let Some(pid) = fields.next().and_then(|field| field.parse::<u32>().ok()) else {
                continue;
            };
            let Some(parent) = fields.next().and_then(|field| field.parse::<u32>().ok()) else {
                continue;
            };
            let args = fields.collect::<Vec<_>>();
            let is_kernel_helper = args
                .iter()
                .any(|arg| matches!(*arg, "__kernel-sentinel" | "__kernel-guard"));
            let belongs_to_fixture =
                parent == self.child.id() || args.contains(&owner_marker.as_str());
            if !is_kernel_helper || !belongs_to_fixture {
                continue;
            }
            match effigy_kernel::ident::process_identity(pid) {
                effigy_kernel::observe::Observed::Absent(_) => {}
                effigy_kernel::observe::Observed::Verified(identity, _) => {
                    if !self.helpers.contains(&identity) {
                        self.helpers.push(identity);
                    }
                }
                effigy_kernel::observe::Observed::Unknown(_) => {
                    return Err(io::Error::other(format!(
                        "test {}: cannot establish Effigy helper identity for PID {pid}",
                        current_test_name().as_deref().unwrap_or("unknown test")
                    )));
                }
            }
        }
        Ok(())
    }

    fn wait_for_helpers(&mut self) -> io::Result<()> {
        // Helper settlement starts when the leader is gone. Charging it to the
        // launch's output deadline made a correct-but-slow run fail teardown on
        // the very deadline its own work had already spent.
        let deadline = self
            .deadline
            .max(Instant::now() + crate::load_scaled_guard(HELPER_SETTLE));
        loop {
            self.capture_kernel_helpers()?;
            self.helpers.retain(|identity| {
                match effigy_kernel::ident::process_identity(identity.pid) {
                    effigy_kernel::observe::Observed::Absent(_) => false,
                    effigy_kernel::observe::Observed::Verified(current, _)
                        if current != *identity =>
                    {
                        false
                    }
                    effigy_kernel::observe::Observed::Verified(..) => !matches!(
                        effigy_kernel::ident::process_state(identity.pid),
                        effigy_kernel::observe::Observed::Verified(
                            effigy_kernel::ident::ProcessState::Zombie,
                            _
                        )
                    ),
                    effigy_kernel::observe::Observed::Unknown(_) => true,
                }
            });
            if self.helpers.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                let pids = self
                    .helpers
                    .iter()
                    .map(|identity| identity.pid)
                    .collect::<Vec<_>>();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "test {}: Effigy helper PID(s) {pids:?} did not exit before the fixture deadline",
                        current_test_name().as_deref().unwrap_or("unknown test")
                    ),
                ));
            }
            std::thread::sleep(POLL);
        }
    }

    fn wait_with_output_inner(mut self) -> io::Result<Output> {
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
            self.capture_kernel_helpers()?;
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
                    self.wait_for_helpers()?;
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

fn current_test_name() -> Option<String> {
    std::thread::current()
        .name()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| std::env::var("NEXTEST_TEST_NAME").ok())
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
            if let Err(error) = self.wait_for_helpers() {
                if std::thread::panicking() {
                    eprintln!("fixture helper cleanup failed: {error}");
                } else {
                    panic!("fixture helper cleanup failed: {error}");
                }
            }
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

#[cfg(test)]
mod tests {
    use super::FixtureCommandExt;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn waits_for_a_kernel_helper_reparented_before_the_cli_exits() {
        let fixture = tempfile::Builder::new()
            .prefix("effigy-helper-wait-")
            .tempdir()
            .expect("fixture directory");
        let executable = fixture.path().join("effigy");
        fs::write(
            &executable,
            "#!/bin/sh\n/bin/sh -c 'sleep 0.1; exec /bin/sh -c \"sleep 0.3\" __kernel-sentinel \"$2\"' delayed \"$$\" \"--effigy-suite-parent=$$\" &\nexit 0\n",
        )
        .expect("fixture executable");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("executable mode");

        let started = Instant::now();
        let status = Command::new(executable)
            .spawn_owned()
            .expect("fixture launch")
            .wait()
            .expect("wait for CLI and its helper");

        assert!(status.success());
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "the owned waiter returned before the reparented helper exited"
        );
    }
}
