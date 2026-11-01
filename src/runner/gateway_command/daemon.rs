use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use effigy_gateway::identity;
use effigy_gateway::server::{self, GatewayConfig};
use effigy_ui::steps::ProgressSource;

use crate::runner::error::RunnerError;

pub(super) fn spawn_gateway_daemon(config: &GatewayConfig) -> Result<(), RunnerError> {
    if let Some(root) = config.private_state_root() {
        effigy_gateway::private_state::validate_root(root)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    } else {
        identity::ensure_trusted_gateway_parent(&config.pid_file_path)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    }
    let effigy_bin = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let stdout_log =
        create_gateway_log(gateway_stdout_log_path(config)).map_err(RunnerError::Cwd)?;
    let stderr_log =
        create_gateway_log(gateway_stderr_log_path(config)).map_err(RunnerError::Cwd)?;
    let mut command = ProcessCommand::new(&effigy_bin);
    command
        .arg("__gateway-run")
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    if let Some(root) = config.private_state_root() {
        command
            .arg("--private-state-root")
            .arg(root)
            .arg("--dns-addr")
            .arg(config.dns.bind_addr.to_string())
            .arg("--proxy-addr")
            .arg(config.proxy.bind_addr.to_string());
        if let Some(address) = config.proxy.tls_bind_addr {
            command.arg("--https-addr").arg(address.to_string());
        }
    }
    super::fixture::configure(&mut command, config).map_err(RunnerError::Cwd)?;
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setsid` is
    // async-signal-safe. On failure, `io::Error::last_os_error()` captures
    // errno without allocating.
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

    #[cfg(debug_assertions)]
    super::fixture::hold_client(config)?;
    Ok(())
}

fn create_gateway_log(path: PathBuf) -> std::io::Result<File> {
    create_gateway_log_with(path, || {})
}

fn create_gateway_log_with(
    path: PathBuf,
    before_child_open: impl FnOnce(),
) -> std::io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let refuse = || std::io::Error::other("gateway log ownership or type is unsafe");
        let parent = path.parent().ok_or_else(refuse)?;
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(parent)?;
        if directory.metadata()?.uid() != nix::unistd::geteuid().as_raw() {
            return Err(refuse());
        }
        directory.set_permissions(std::fs::Permissions::from_mode(0o700))?;
        let name = path.file_name().ok_or_else(refuse)?;
        before_child_open();
        open_gateway_log_at(&directory, name)
    }
    #[cfg(not(unix))]
    {
        before_child_open();
        OpenOptions::new().create(true).append(true).open(path)
    }
}

#[cfg(unix)]
fn open_gateway_log_at(directory: &File, name: &std::ffi::OsStr) -> std::io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    };
    let refuse = || std::io::Error::other("gateway log ownership or type is unsafe");
    if name.as_bytes().is_empty()
        || name.as_bytes().contains(&b'/')
        || matches!(name.as_bytes(), b"." | b"..")
    {
        return Err(refuse());
    }
    let name = std::ffi::CString::new(name.as_bytes()).map_err(|_| refuse())?;
    // SAFETY: both descriptors and the NUL-terminated single component remain
    // valid for openat. O_NONBLOCK prevents a FIFO from blocking before fstat.
    let raw = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY
                | libc::O_APPEND
                | libc::O_CREAT
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | libc::O_NONBLOCK,
            0o600,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: successful openat returned a new exclusively owned descriptor.
    let file = unsafe { File::from_raw_fd(raw) };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != nix::unistd::geteuid().as_raw()
    {
        return Err(refuse());
    }
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

pub(super) fn wait_for_private_gateway_ready(
    config: &GatewayConfig,
    progress: &ProgressSource,
) -> Result<(), RunnerError> {
    let operation = effigy_gateway::operator::context(
        Duration::from_secs(15),
        effigy_kernel::ctx::EffectClass::Observe,
    );
    // The listener address lives in the probe's own sub-step: once an event
    // arrives the frame shows that event instead of this sentence, and the
    // operator must still see the exact port being waited on.
    progress.now_waiting_for("the gateway to answer");
    wait_for_private_gateway_ready_with_probe(&operation, progress, config.proxy.bind_addr, || {
        server::get_verified_gateway_status(config).map(|_| ())
    })
}

/// Poll the verified listener until it answers or the budget ends. Each
/// refused probe names its blocker as the step's last event, and the listener
/// address stays in the current sub-step so no frame loses the exact port.
pub(super) fn wait_for_private_gateway_ready_with_probe(
    operation: &effigy_kernel::ctx::OperationContext,
    progress: &ProgressSource,
    listener: std::net::SocketAddr,
    mut probe: impl FnMut() -> Result<(), effigy_gateway::GatewayError>,
) -> Result<(), RunnerError> {
    let action = format!("checking the verified listener on {listener}");
    progress.update(action.as_str(), None, None);
    loop {
        operation
            .check("private gateway verified listener publication")
            .map_err(|error| {
                RunnerError::task_invocation(format!(
                    "private gateway startup interrupted: {error}; the daemon was not confirmed ready"
                ))
            })?;
        let blocker = match probe() {
            Ok(()) => return Ok(()),
            Err(effigy_gateway::GatewayError::NotRunning) => "the gateway is not answering yet",
            Err(effigy_gateway::GatewayError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                "the gateway record is not published yet"
            }
            Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
        };
        progress.update(action.as_str(), None, Some(blocker.into()));
        let remaining = operation.remaining().ok_or_else(|| {
            RunnerError::task_invocation("private gateway startup requires a finite budget")
        })?;
        thread::sleep(remaining.min(Duration::from_millis(25)));
    }
}

pub(super) fn stop_gateway_process(
    snapshot: &effigy_gateway::identity::GatewayRecordSnapshot,
) -> Result<(), RunnerError> {
    stop_gateway_process_with_probe(snapshot, super::gateway_identity_probe)
}

pub(super) fn stop_private_gateway_process(
    snapshot: &effigy_gateway::identity::GatewayRecordSnapshot,
) -> Result<(), RunnerError> {
    stop_gateway_process_with_probe(snapshot, |record, _| identity::probe_live_identity(record))
}

pub(super) fn stop_gateway_process_for_replacement(
    snapshot: &effigy_gateway::identity::GatewayRecordSnapshot,
) -> Result<(), RunnerError> {
    stop_gateway_process_with_probe(snapshot, |record, snapshot| {
        super::gateway_identity_probe_with(
            record,
            snapshot,
            identity::probe_live_identity,
            |_, _, _| None,
        )
    })
}

fn stop_gateway_process_with_probe(
    snapshot: &effigy_gateway::identity::GatewayRecordSnapshot,
    mut identity_probe: impl FnMut(
        &identity::GatewayIdentityRecord,
        &effigy_gateway::identity::GatewayRecordSnapshot,
    ) -> effigy_gateway::identity::GatewayIdentityProbe,
) -> Result<(), RunnerError> {
    let Some(record) = snapshot.record() else {
        return Err(RunnerError::task_invocation(
            "gateway identity is missing; refusing to signal",
        ));
    };
    // The action that asked for this stop recorded itself when it started, so
    // this helper only signals; it never adds a second event for one action.
    let pid = snapshot.pid();
    #[cfg(unix)]
    {
        stop_gateway_process_with_identity(
            pid,
            server::probe_gateway_process,
            || identity_probe(record, snapshot),
            |pid_t, signal| {
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid_t), signal)
                    .map_err(|error| error.to_string())
            },
            thread::sleep,
        )
    }

    #[cfg(not(unix))]
    {
        checked_gateway_signal_pid(pid)?;
        Err(RunnerError::task_invocation(
            "`effigy gateway down` is not implemented on this host platform yet",
        ))
    }
}

fn checked_gateway_signal_pid(pid: u32) -> Result<i32, RunnerError> {
    let Some(pid_t) = server::checked_gateway_pid(pid) else {
        return Err(invalid_gateway_pid(pid));
    };
    if !gateway_signal_target_is_safe(pid_t) {
        return Err(invalid_gateway_pid(pid));
    }
    Ok(pid_t)
}

fn gateway_signal_target_is_safe(pid_t: i32) -> bool {
    pid_t > 1 && u32::try_from(pid_t).ok() != Some(std::process::id())
}

fn invalid_gateway_pid(pid: u32) -> RunnerError {
    RunnerError::task_invocation(format!("invalid gateway PID {pid}"))
}

#[cfg(unix)]
fn send_gateway_signal_with(
    pid: u32,
    signal: nix::sys::signal::Signal,
    dispatch: impl FnOnce(i32, nix::sys::signal::Signal) -> Result<(), String>,
) -> Result<(), RunnerError> {
    let pid_t = checked_gateway_signal_pid(pid)?;
    dispatch(pid_t, signal).map_err(RunnerError::task_invocation)
}

#[cfg(unix)]
#[cfg(test)]
fn stop_gateway_process_with(
    pid: u32,
    process_state: impl FnMut(u32) -> server::GatewayProcessProbe,
    dispatch: impl FnMut(i32, nix::sys::signal::Signal) -> Result<(), String>,
    wait: impl FnMut(Duration),
) -> Result<(), RunnerError> {
    stop_gateway_process_with_identity(
        pid,
        process_state,
        || effigy_gateway::identity::GatewayIdentityProbe::Matched,
        dispatch,
        wait,
    )
}

#[cfg(unix)]
fn stop_gateway_process_with_identity(
    pid: u32,
    mut process_state: impl FnMut(u32) -> server::GatewayProcessProbe,
    mut generation_state: impl FnMut() -> effigy_gateway::identity::GatewayIdentityProbe,
    mut dispatch: impl FnMut(i32, nix::sys::signal::Signal) -> Result<(), String>,
    mut wait: impl FnMut(Duration),
) -> Result<(), RunnerError> {
    // Validate before even the first liveness probe; callers may bypass the
    // PID-file reader and this function owns the signal boundary.
    checked_gateway_signal_pid(pid)?;
    match process_generation_state(pid, &mut process_state, &mut generation_state) {
        ProcessGenerationState::Absent => return Ok(()),
        ProcessGenerationState::Running => {}
        ProcessGenerationState::Unknown => {
            if process_state(pid) == server::GatewayProcessProbe::ConfirmedAbsent {
                return Ok(());
            }
            return Err(gateway_process_state_unknown(pid));
        }
    }

    // Recheck the persisted generation immediately before the TERM dispatch.
    match generation_state() {
        effigy_gateway::identity::GatewayIdentityProbe::Matched => {}
        effigy_gateway::identity::GatewayIdentityProbe::Mismatch => return Ok(()),
        effigy_gateway::identity::GatewayIdentityProbe::PermissionDenied
        | effigy_gateway::identity::GatewayIdentityProbe::Unknown => {
            if process_state(pid) == server::GatewayProcessProbe::ConfirmedAbsent {
                return Ok(());
            }
            return Err(gateway_process_state_unknown(pid));
        }
    }
    if let Err(error) =
        send_gateway_signal_with(pid, nix::sys::signal::Signal::SIGTERM, |pid_t, signal| {
            dispatch(pid_t, signal)
        })
    {
        if process_state(pid) == server::GatewayProcessProbe::ConfirmedAbsent {
            return Ok(());
        }
        return Err(error);
    }
    for _ in 0..40 {
        match process_generation_state(pid, &mut process_state, &mut generation_state) {
            ProcessGenerationState::Absent => return Ok(()),
            ProcessGenerationState::Running | ProcessGenerationState::Unknown => {
                wait(Duration::from_millis(50));
            }
        }
    }

    match process_generation_state(pid, &mut process_state, &mut generation_state) {
        ProcessGenerationState::Absent => return Ok(()),
        ProcessGenerationState::Running => {}
        ProcessGenerationState::Unknown => return Err(gateway_process_state_unknown(pid)),
    }

    // Recheck the recorded identity again immediately before escalation.
    match generation_state() {
        effigy_gateway::identity::GatewayIdentityProbe::Matched => {}
        effigy_gateway::identity::GatewayIdentityProbe::Mismatch => return Ok(()),
        effigy_gateway::identity::GatewayIdentityProbe::PermissionDenied
        | effigy_gateway::identity::GatewayIdentityProbe::Unknown => {
            if process_state(pid) == server::GatewayProcessProbe::ConfirmedAbsent {
                return Ok(());
            }
            return Err(gateway_process_state_unknown(pid));
        }
    }
    if let Err(error) =
        send_gateway_signal_with(pid, nix::sys::signal::Signal::SIGKILL, |pid_t, signal| {
            dispatch(pid_t, signal)
        })
    {
        if process_state(pid) == server::GatewayProcessProbe::ConfirmedAbsent {
            return Ok(());
        }
        return Err(error);
    }
    for _ in 0..20 {
        match process_generation_state(pid, &mut process_state, &mut generation_state) {
            ProcessGenerationState::Absent => return Ok(()),
            ProcessGenerationState::Running | ProcessGenerationState::Unknown => {
                wait(Duration::from_millis(50));
            }
        }
    }

    match process_generation_state(pid, &mut process_state, &mut generation_state) {
        ProcessGenerationState::Absent => Ok(()),
        ProcessGenerationState::Unknown => Err(gateway_process_state_unknown(pid)),
        ProcessGenerationState::Running => Err(RunnerError::task_invocation(format!(
            "gateway process {pid} did not stop after SIGTERM/SIGKILL"
        ))),
    }
}

#[cfg(all(test, unix))]
pub(super) fn stop_gateway_process_with_test_control(
    pid: u32,
    process_state: impl FnMut(u32) -> server::GatewayProcessProbe,
    generation_state: impl FnMut() -> effigy_gateway::identity::GatewayIdentityProbe,
    dispatch: impl FnMut(i32, nix::sys::signal::Signal) -> Result<(), String>,
    wait: impl FnMut(Duration),
) -> Result<(), RunnerError> {
    stop_gateway_process_with_identity(pid, process_state, generation_state, dispatch, wait)
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessGenerationState {
    Running,
    Absent,
    Unknown,
}

#[cfg(unix)]
fn process_generation_state(
    pid: u32,
    process_state: &mut impl FnMut(u32) -> server::GatewayProcessProbe,
    generation_state: &mut impl FnMut() -> effigy_gateway::identity::GatewayIdentityProbe,
) -> ProcessGenerationState {
    match process_state(pid) {
        server::GatewayProcessProbe::ConfirmedAbsent => ProcessGenerationState::Absent,
        server::GatewayProcessProbe::Unknown => ProcessGenerationState::Unknown,
        server::GatewayProcessProbe::Running => match generation_state() {
            effigy_gateway::identity::GatewayIdentityProbe::Matched => {
                ProcessGenerationState::Running
            }
            effigy_gateway::identity::GatewayIdentityProbe::Mismatch => {
                ProcessGenerationState::Absent
            }
            effigy_gateway::identity::GatewayIdentityProbe::PermissionDenied
            | effigy_gateway::identity::GatewayIdentityProbe::Unknown => {
                ProcessGenerationState::Unknown
            }
        },
    }
}

#[cfg(unix)]
fn gateway_process_state_unknown(pid: u32) -> RunnerError {
    RunnerError::task_invocation(format!(
        "cannot determine whether gateway process {pid} is running; refusing to report it stopped"
    ))
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

#[cfg(test)]
mod pid_domain_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn gateway_pid_domain_rejects_invalid_targets_before_probe_or_signal() {
        use std::cell::Cell;

        let probes = Cell::new(0);
        let signals = Cell::new(0);
        for pid in [0, 1, i32::MAX as u32 + 1, u32::MAX, std::process::id()] {
            let result = stop_gateway_process_with(
                pid,
                |_| {
                    probes.set(probes.get() + 1);
                    server::GatewayProcessProbe::Running
                },
                |_, _| {
                    signals.set(signals.get() + 1);
                    Ok(())
                },
                |_| {},
            );
            assert!(result.is_err(), "PID {pid} must be refused");
        }
        assert_eq!(probes.get(), 0);
        assert_eq!(signals.get(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_mismatch_or_unknown_dispatches_no_signal() {
        use std::cell::Cell;

        for generation in [
            effigy_gateway::identity::GatewayIdentityProbe::Mismatch,
            effigy_gateway::identity::GatewayIdentityProbe::Unknown,
        ] {
            let signals = Cell::new(0);
            let result = stop_gateway_process_with_identity(
                4242,
                |_| server::GatewayProcessProbe::Running,
                || generation,
                |_, _| {
                    signals.set(signals.get() + 1);
                    Ok(())
                },
                |_| {},
            );
            assert_eq!(signals.get(), 0);
            if generation == effigy_gateway::identity::GatewayIdentityProbe::Mismatch {
                assert!(result.is_ok(), "a readable different generation is gone");
            } else {
                assert!(result.is_err(), "unknown identity refuses signals");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_legacy_stops_matched_term_resistant_owned_child() {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        use std::cell::Cell;
        use std::io::{BufRead, BufReader};
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;
        use std::process::{Child, Command};
        use std::time::Instant;

        struct OwnedChild(Child);

        impl Drop for OwnedChild {
            fn drop(&mut self) {
                if self.0.try_wait().ok().flatten().is_none() {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                }
            }
        }

        fn spawn_private_child(script: &str) -> (OwnedChild, UnixStream) {
            let (ready_reader, ready_writer) =
                UnixStream::pair().expect("create private readiness socket");
            let ready_fd = ready_writer.as_raw_fd();
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            // SAFETY: dup2 is async-signal-safe and only installs the child's
            // private readiness socket at fd 3 before exec.
            unsafe {
                command.pre_exec(move || {
                    if nix::libc::dup2(ready_fd, 3) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let child = OwnedChild(command.spawn().expect("start private gateway child"));
            drop(ready_writer);
            (child, ready_reader)
        }

        fn await_term_resistance(
            child: &mut OwnedChild,
            ready_reader: &mut UnixStream,
            timeout: Duration,
        ) -> Result<(), String> {
            let pid = child.0.id();
            ready_reader
                .set_read_timeout(Some(timeout))
                .map_err(|error| format!("set TERM-resistance readiness timeout: {error}"))?;
            let mut acknowledgment = String::new();
            match BufReader::new(ready_reader).read_line(&mut acknowledgment) {
                Ok(0) => {
                    let exit_deadline = Instant::now() + timeout;
                    loop {
                        if let Some(status) = child
                            .0
                            .try_wait()
                            .map_err(|error| format!("check child before readiness: {error}"))?
                        {
                            return Err(format!(
                                "child exited with status {status} before TERM-resistance readiness"
                            ));
                        }
                        if Instant::now() >= exit_deadline {
                            return Err(
                                "readiness channel closed before TERM-resistance acknowledgment; child did not exit"
                                    .to_owned(),
                            );
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                }
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if let Some(status) = child.0.try_wait().map_err(|wait_error| {
                        format!("check child at readiness timeout: {wait_error}")
                    })? {
                        return Err(format!(
                            "child exited with status {status} before TERM-resistance readiness"
                        ));
                    }
                    return Err(format!(
                        "TERM-resistance readiness timed out after {timeout:?}"
                    ));
                }
                Err(error) => {
                    return Err(format!("read TERM-resistance acknowledgment: {error}"));
                }
            }

            let expected = format!("ARMED:{pid}\n");
            if acknowledgment != expected {
                return Err(format!(
                    "invalid TERM-resistance acknowledgment {acknowledgment:?}; expected {expected:?}"
                ));
            }
            if let Some(status) = child
                .0
                .try_wait()
                .map_err(|error| format!("check child after readiness: {error}"))?
            {
                return Err(format!(
                    "child exited with status {status} after TERM-resistance acknowledgment"
                ));
            }
            if server::probe_gateway_process(pid) != server::GatewayProcessProbe::Running {
                return Err(format!(
                    "child {pid} acknowledged TERM resistance but is not running"
                ));
            }
            Ok(())
        }

        let (mut child, mut readiness) =
            spawn_private_child("trap '' TERM; printf 'ARMED:%s\\n' \"$$\" >&3; exec sleep 30");
        let pid = child.0.id();
        await_term_resistance(&mut child, &mut readiness, Duration::from_secs(5))
            .expect("child must acknowledge TERM resistance before escalation proof");
        let mut signals = Vec::new();
        let identity_reads = Cell::new(0);
        let result = stop_gateway_process_with_identity(
            pid,
            server::probe_gateway_process,
            || {
                identity_reads.set(identity_reads.get() + 1);
                effigy_gateway::identity::GatewayIdentityProbe::Matched
            },
            |pid_t, signal| {
                signals.push(signal);
                kill(Pid::from_raw(pid_t), signal).map_err(|error| error.to_string())
            },
            thread::sleep,
        );
        result.expect("stop only this matched, owned child");
        assert_eq!(signals, [Signal::SIGTERM, Signal::SIGKILL]);
        assert_eq!(
            identity_reads.get(),
            44,
            "identity is reread at every poll and signal boundary"
        );
        assert!(child.0.try_wait().expect("reap child").is_some());
        assert_eq!(
            server::probe_gateway_process(pid),
            server::GatewayProcessProbe::ConfirmedAbsent
        );

        // A running process without the post-trap acknowledgment cannot enter
        // the escalation oracle. Its owned-child guard must still clean it up.
        let (mut unarmed_child, mut unarmed_readiness) = spawn_private_child("exec sleep 30");
        let unarmed_pid = unarmed_child.0.id();
        let unarmed_result = await_term_resistance(
            &mut unarmed_child,
            &mut unarmed_readiness,
            Duration::from_secs(1),
        );
        let mut unarmed_signals = Vec::new();
        if unarmed_result.is_ok() {
            let _ = stop_gateway_process_with_identity(
                unarmed_pid,
                server::probe_gateway_process,
                || effigy_gateway::identity::GatewayIdentityProbe::Matched,
                |pid_t, signal| {
                    unarmed_signals.push(signal);
                    kill(Pid::from_raw(pid_t), signal).map_err(|error| error.to_string())
                },
                thread::sleep,
            );
        }
        assert!(
            unarmed_result
                .as_ref()
                .is_err_and(|error| error.contains("readiness timed out")),
            "running unarmed child must fail readiness: {unarmed_result:?}"
        );
        assert!(
            unarmed_signals.is_empty(),
            "unarmed child must not reach escalation"
        );
        drop(unarmed_child);
        assert_eq!(
            server::probe_gateway_process(unarmed_pid),
            server::GatewayProcessProbe::ConfirmedAbsent,
            "readiness failure must clean up and reap its exact owned child"
        );

        let (mut exited_child, mut exited_readiness) = spawn_private_child("exit 7");
        let exited_pid = exited_child.0.id();
        let exited_result = await_term_resistance(
            &mut exited_child,
            &mut exited_readiness,
            Duration::from_secs(1),
        );
        assert!(
            exited_result
                .as_ref()
                .is_err_and(|error| error.contains("exited with status")),
            "early child exit must be reported before escalation: {exited_result:?}"
        );
        drop(exited_child);
        assert_eq!(
            server::probe_gateway_process(exited_pid),
            server::GatewayProcessProbe::ConfirmedAbsent,
            "early-exit readiness failure must reap its exact owned child"
        );
    }

    #[cfg(unix)]
    #[test]
    fn gateway_pid_domain_rejects_invalid_direct_signal_dispatch() {
        use std::cell::Cell;

        let dispatches = Cell::new(0);
        for pid in [0, 1, i32::MAX as u32 + 1, u32::MAX, std::process::id()] {
            let result =
                send_gateway_signal_with(pid, nix::sys::signal::Signal::SIGTERM, |_, _| {
                    dispatches.set(dispatches.get() + 1);
                    Ok(())
                });
            assert!(result.is_err(), "PID {pid} must be refused");
        }
        assert_eq!(dispatches.get(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_pid_domain_already_stopped_target_is_idempotent() {
        use std::cell::Cell;

        let signals = Cell::new(0);
        let result = stop_gateway_process_with(
            i32::MAX as u32,
            |_| server::GatewayProcessProbe::ConfirmedAbsent,
            |_, _| {
                signals.set(signals.get() + 1);
                Ok(())
            },
            |_| {},
        );
        result.expect("already-stopped gateway target should be idempotent");
        assert_eq!(signals.get(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_probe_state_unknown_before_stop_dispatches_no_signal() {
        use std::cell::Cell;

        let signals = Cell::new(0);
        let result = stop_gateway_process_with(
            i32::MAX as u32,
            |_| server::GatewayProcessProbe::Unknown,
            |_, _| {
                signals.set(signals.get() + 1);
                Ok(())
            },
            |_| {},
        );
        result.expect_err("an unknown probe must not report a successful stop");
        assert_eq!(signals.get(), 0, "unknown-before-stop must not dispatch");
    }

    #[cfg(unix)]
    #[test]
    fn gateway_probe_state_unknown_after_term_refuses_success_without_kill() {
        use std::cell::Cell;

        let probes = Cell::new(0);
        let signals = Cell::new(0);
        let result = stop_gateway_process_with(
            i32::MAX as u32,
            |_| {
                let previous = probes.get();
                probes.set(previous + 1);
                // Running before TERM, then ambiguous on every later probe.
                if previous == 0 {
                    server::GatewayProcessProbe::Running
                } else {
                    server::GatewayProcessProbe::Unknown
                }
            },
            |_, _| {
                signals.set(signals.get() + 1);
                Ok(())
            },
            |_| {},
        );
        result.expect_err("unknown after TERM must not report success");
        assert_eq!(
            signals.get(),
            1,
            "only the initial SIGTERM may be dispatched"
        );
        // The reconciliation window is bounded: an ambiguous probe stops the
        // loop well short of an unbounded retry, whatever its exact length.
        assert!(
            (1..=64).contains(&probes.get()),
            "ambiguous post-TERM probes must stay bounded: {}",
            probes.get()
        );
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_legacy_transient_unknown_after_term_reconciles_confirmed_absence() {
        use nix::sys::signal::Signal;
        use std::cell::{Cell, RefCell};

        let probes = Cell::new(0);
        let generations = Cell::new(0);
        let signals = RefCell::new(Vec::new());
        let result = stop_gateway_process_with_identity(
            4242,
            |_| {
                let probe = probes.get();
                probes.set(probe + 1);
                match probe {
                    0 => server::GatewayProcessProbe::Running,
                    1 => server::GatewayProcessProbe::Unknown,
                    _ => server::GatewayProcessProbe::ConfirmedAbsent,
                }
            },
            || {
                generations.set(generations.get() + 1);
                effigy_gateway::identity::GatewayIdentityProbe::Matched
            },
            |_, signal| {
                signals.borrow_mut().push(signal);
                Ok(())
            },
            |_| {},
        );

        result.expect("a later exact absence probe should reconcile the stopped generation");
        assert_eq!(*signals.borrow(), [Signal::SIGTERM]);
        // One process probe and one identity read per observation, repeated
        // only until the absence is confirmed.
        assert!(probes.get() >= 2, "probes: {}", probes.get());
        assert!(generations.get() >= 2, "generations: {}", generations.get());
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_legacy_exit_before_term_reconciles_only_after_absence_confirmation() {
        use std::cell::{Cell, RefCell};

        let probes = Cell::new(0);
        let generations = Cell::new(0);
        let signals = RefCell::new(Vec::new());
        let result = stop_gateway_process_with_identity(
            4242,
            |_| {
                probes.set(probes.get() + 1);
                if probes.get() == 1 {
                    server::GatewayProcessProbe::Running
                } else {
                    server::GatewayProcessProbe::ConfirmedAbsent
                }
            },
            || {
                generations.set(generations.get() + 1);
                if generations.get() == 1 {
                    effigy_gateway::identity::GatewayIdentityProbe::Matched
                } else {
                    effigy_gateway::identity::GatewayIdentityProbe::Unknown
                }
            },
            |_, signal| {
                signals.borrow_mut().push(signal);
                Ok(())
            },
            |_| {},
        );

        result.expect("confirmed absence after the final identity read permits reconciliation");
        assert!(
            signals.borrow().is_empty(),
            "TERM must not target an exited PID"
        );
        assert_eq!(probes.get(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_identity_legacy_term_race_reconciles_only_after_absence_confirmation() {
        use nix::sys::signal::Signal;
        use std::cell::{Cell, RefCell};

        let probes = Cell::new(0);
        let signals = RefCell::new(Vec::new());
        let result = stop_gateway_process_with_identity(
            4242,
            |_| {
                probes.set(probes.get() + 1);
                if probes.get() == 1 {
                    server::GatewayProcessProbe::Running
                } else {
                    server::GatewayProcessProbe::ConfirmedAbsent
                }
            },
            || effigy_gateway::identity::GatewayIdentityProbe::Matched,
            |_, signal| {
                signals.borrow_mut().push(signal);
                Err("target exited before signal delivery".to_owned())
            },
            |_| {},
        );

        result.expect("confirmed absence after failed TERM permits reconciliation");
        assert_eq!(*signals.borrow(), [Signal::SIGTERM]);
        assert_eq!(probes.get(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn gateway_pid_domain_stops_exact_private_owned_child() {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        use std::process::{Child, Command};

        struct OwnedChild(Child);

        impl Drop for OwnedChild {
            fn drop(&mut self) {
                if self.0.try_wait().ok().flatten().is_none() {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                }
            }
        }

        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("start private owned child");
        let mut child = OwnedChild(child);
        let pid = child.0.id();
        let result = stop_gateway_process_with(
            pid,
            server::probe_gateway_process,
            |pid_t, signal| kill(Pid::from_raw(pid_t), signal).map_err(|error| error.to_string()),
            thread::sleep,
        );
        result.expect("stop exact private owned child");
        assert!(child.0.try_wait().expect("reap child").is_some());
        assert_eq!(
            server::probe_gateway_process(pid),
            server::GatewayProcessProbe::ConfirmedAbsent
        );
    }
}

#[cfg(all(test, unix))]
mod log_permission_tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn gateway_raw_log_reopens_with_private_permissions_and_refuses_aliases() {
        let fixture = effigy_test_support::Fixture::new("gateway-log-private").expect("fixture");
        let directory = fixture.path().join("gateway");
        std::fs::create_dir(&directory).expect("directory");
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755))
            .expect("old directory mode");
        let path = directory.join("gateway.stderr.log");
        std::fs::write(&path, b"legacy output").expect("old log");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("old log mode");
        drop(create_gateway_log(path.clone()).expect("private reopen"));
        assert_eq!(
            std::fs::metadata(&directory)
                .expect("directory")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).expect("log").permissions().mode() & 0o777,
            0o600
        );
        let alias = directory.join("gateway.stdout.log");
        symlink(&path, &alias).expect("symlink control");
        assert!(create_gateway_log(alias).is_err());
        let alias = directory.join("hardlink.log");
        std::fs::hard_link(&path, &alias).expect("hardlink control");
        assert!(create_gateway_log(alias).is_err());
        assert_eq!(
            std::fs::read(&path).expect("original log"),
            b"legacy output"
        );
    }
}

#[cfg(all(test, unix))]
mod log_open_race_tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::{ffi::OsStrExt, fs::symlink};

    #[test]
    fn gateway_log_child_open_stays_in_verified_directory_after_replacement() {
        for ancestor in [false, true] {
            let fixture =
                effigy_test_support::Fixture::new("gateway-log-replaced").expect("fixture");
            let outer = fixture.path().join("outer");
            let original = outer.join("gateway");
            std::fs::create_dir_all(&original).expect("original");
            let replacement = fixture.path().join("replacement");
            std::fs::create_dir(&replacement).expect("replacement");
            if ancestor {
                std::fs::create_dir(replacement.join("gateway")).expect("replacement child");
            }
            let moved = fixture.path().join("retired");
            let mut file = create_gateway_log_with(original.join("gateway.stderr.log"), || {
                let changed = if ancestor { &outer } else { &original };
                std::fs::rename(changed, &moved).expect("move verified directory");
                symlink(&replacement, changed).expect("replace pathname");
            })
            .expect("anchored log");
            file.write_all(b"anchored").expect("write");
            let log = if ancestor {
                moved.join("gateway/gateway.stderr.log")
            } else {
                moved.join("gateway.stderr.log")
            };
            assert_eq!(std::fs::read(log).expect("original inode"), b"anchored");
            assert!(!replacement.join("gateway.stderr.log").exists());
            assert!(!replacement.join("gateway/gateway.stderr.log").exists());
        }
    }

    #[test]
    fn gateway_log_fifo_and_device_are_refused_without_blocking_or_chmod() {
        let fixture = effigy_test_support::Fixture::new("gateway-log-special").expect("fixture");
        let fifo = fixture.path().join("gateway.stderr.log");
        let name = std::ffi::CString::new(fifo.as_os_str().as_bytes()).expect("path");
        // SAFETY: the pathname is valid and names a new private fixture entry.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        assert!(create_gateway_log(fifo).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        let directory = File::open("/dev").expect("device directory");
        let before = std::fs::metadata("/dev/null").expect("device metadata");
        assert!(open_gateway_log_at(&directory, std::ffi::OsStr::new("null")).is_err());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata("/dev/null")
                .expect("device unchanged")
                .permissions()
                .mode(),
            before.permissions().mode()
        );
    }
}
