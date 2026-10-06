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

pub(super) fn stop_gateway_process(
    snapshot: &effigy_gateway::identity::GatewayRecordSnapshot,
) -> Result<(), RunnerError> {
    let Some(record) = snapshot.record() else {
        return Err(RunnerError::task_invocation(
            "gateway identity is missing; refusing to signal",
        ));
    };
    let pid = snapshot.pid();
    #[cfg(unix)]
    {
        stop_gateway_process_with_identity(
            pid,
            server::probe_gateway_process,
            || super::gateway_identity_probe(record, snapshot),
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
    if !check_process_generation(pid, &mut process_state, &mut generation_state)? {
        return Ok(());
    }

    // Recheck the persisted generation immediately before the TERM dispatch.
    match generation_state() {
        effigy_gateway::identity::GatewayIdentityProbe::Matched => {}
        effigy_gateway::identity::GatewayIdentityProbe::Mismatch => return Ok(()),
        effigy_gateway::identity::GatewayIdentityProbe::PermissionDenied
        | effigy_gateway::identity::GatewayIdentityProbe::Unknown => {
            return Err(gateway_process_state_unknown(pid));
        }
    }
    send_gateway_signal_with(pid, nix::sys::signal::Signal::SIGTERM, |pid_t, signal| {
        dispatch(pid_t, signal)
    })?;
    for _ in 0..40 {
        match check_process_generation(pid, &mut process_state, &mut generation_state)? {
            false => return Ok(()),
            true => wait(Duration::from_millis(50)),
        }
    }

    // Recheck the recorded identity again immediately before escalation.
    match generation_state() {
        effigy_gateway::identity::GatewayIdentityProbe::Matched => {}
        effigy_gateway::identity::GatewayIdentityProbe::Mismatch => return Ok(()),
        effigy_gateway::identity::GatewayIdentityProbe::PermissionDenied
        | effigy_gateway::identity::GatewayIdentityProbe::Unknown => {
            return Err(gateway_process_state_unknown(pid));
        }
    }
    send_gateway_signal_with(pid, nix::sys::signal::Signal::SIGKILL, |pid_t, signal| {
        dispatch(pid_t, signal)
    })?;
    for _ in 0..20 {
        match check_process_generation(pid, &mut process_state, &mut generation_state)? {
            false => return Ok(()),
            true => wait(Duration::from_millis(50)),
        }
    }

    Err(RunnerError::task_invocation(format!(
        "gateway process {pid} did not stop after SIGTERM/SIGKILL"
    )))
}

#[cfg(unix)]
fn check_process_generation(
    pid: u32,
    process_state: &mut impl FnMut(u32) -> server::GatewayProcessProbe,
    generation_state: &mut impl FnMut() -> effigy_gateway::identity::GatewayIdentityProbe,
) -> Result<bool, RunnerError> {
    match process_state(pid) {
        server::GatewayProcessProbe::ConfirmedAbsent => Ok(false),
        server::GatewayProcessProbe::Unknown => Err(gateway_process_state_unknown(pid)),
        server::GatewayProcessProbe::Running => match generation_state() {
            effigy_gateway::identity::GatewayIdentityProbe::Matched => Ok(true),
            effigy_gateway::identity::GatewayIdentityProbe::Mismatch => Ok(false),
            effigy_gateway::identity::GatewayIdentityProbe::PermissionDenied
            | effigy_gateway::identity::GatewayIdentityProbe::Unknown => {
                Err(gateway_process_state_unknown(pid))
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
    fn gateway_identity_stops_matched_term_resistant_owned_child() {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
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

        let child = Command::new("sh")
            .args(["-c", "trap '' TERM; exec sleep 30"])
            .spawn()
            .expect("start private TERM-resistant child");
        let mut child = OwnedChild(child);
        let pid = child.0.id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while server::probe_gateway_process(pid) != server::GatewayProcessProbe::Running {
            assert!(Instant::now() < deadline, "child readiness timeout");
            thread::sleep(Duration::from_millis(10));
        }
        let mut signals = Vec::new();
        let result = stop_gateway_process_with_identity(
            pid,
            server::probe_gateway_process,
            || effigy_gateway::identity::GatewayIdentityProbe::Matched,
            |pid_t, signal| {
                signals.push(signal);
                kill(Pid::from_raw(pid_t), signal).map_err(|error| error.to_string())
            },
            thread::sleep,
        );
        result.expect("stop only this matched, owned child");
        assert_eq!(signals, [Signal::SIGTERM, Signal::SIGKILL]);
        assert!(child.0.try_wait().expect("reap child").is_some());
        assert_eq!(
            server::probe_gateway_process(pid),
            server::GatewayProcessProbe::ConfirmedAbsent
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
    fn gateway_pid_domain_unchecked_cast_negative_control_fails_signal_oracle() {
        let old_unchecked_target = u32::MAX as i32;
        assert_eq!(old_unchecked_target, -1);
        assert!(!gateway_signal_target_is_safe(old_unchecked_target));
        assert!(checked_gateway_signal_pid(u32::MAX).is_err());
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
        assert_eq!(probes.get(), 2, "the first post-TERM probe is unknown");
    }

    #[cfg(unix)]
    #[test]
    fn gateway_probe_state_negative_bool_collapse_fails_stop_success_oracle() {
        use std::cell::Cell;

        // The old bool probe returned `false` for an unavailable probe, which
        // `stop_gateway_process_with` treated as a successful stop. The
        // tri-state path must fail instead of reporting success.
        let old_bool_result = !server::GatewayProcessProbe::Unknown.is_running();
        assert!(
            old_bool_result,
            "the compatibility bool still collapses unknown to not-running"
        );

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
        assert!(
            result.is_err(),
            "collapsing unknown to not-running must fail the stop oracle"
        );
        assert_eq!(signals.get(), 0);
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
