//! A plain task command under kernel custody (contract 052 §5.7, K6, K9).
//!
//! The command always gets its own process group behind the spawn barrier,
//! with a durable custody record before it executes. Signals reach this
//! process through the one router and are forwarded to that group only. The
//! run's settle verifies the group closed, so nothing the task started
//! outlives the run. Text and JSON differ only in whether stdout and stderr
//! are captured (K11).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{OpenReason, StopCause};
use effigy_kernel::custody::{Custody, EnvValue, Lifetime, ProcessSpec, SecretValue, Terminal};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::WorkloadOutcome;

use super::kernel::{self, CustodyWait};
use super::run::TaskRun;
use crate::runner::error::RunnerError;

/// TERM to KILL for the task's group at settle or on a forwarded signal.
const GRACE: Duration = Duration::from_secs(3);
/// After a forwarded signal, how long the group gets to exit by itself.
const SIGNAL_EXIT_BUDGET: Duration = Duration::from_secs(10);
/// JSON output keeps this much of each stream, newest bytes last.
const CAPTURE_TAIL: usize = 8 << 20;
/// Exit, signal and capture polling interval for a waiting owner.
const POLL: Duration = Duration::from_millis(20);

pub(in crate::runner) struct PlainCommand<'a> {
    pub(in crate::runner) command: &'a str,
    pub(in crate::runner) cwd: &'a Path,
    /// Overrides on top of this process's environment. Secrets stay out of
    /// argv and are redacted from any capture (K12).
    pub(in crate::runner) env: Vec<(String, EnvOverride)>,
    pub(in crate::runner) capture: bool,
    pub(in crate::runner) timeout_ms: Option<u64>,
}

/// A spawn-environment override carried as a secret: redacted from every
/// capture and never placed in argv.
#[derive(Clone)]
pub(in crate::runner) enum EnvOverride {
    Plain(String),
    Secret(String),
}

#[derive(Debug)]
pub(in crate::runner) struct PlainExit {
    pub(in crate::runner) workload: WorkloadOutcome,
    /// The task's exit code; `128 + n` when a signal ended it.
    pub(in crate::runner) code: i32,
    pub(in crate::runner) stdout: Vec<u8>,
    pub(in crate::runner) stderr: Vec<u8>,
}

pub(in crate::runner) fn run_plain(
    run: &mut TaskRun,
    command: PlainCommand<'_>,
) -> Result<PlainExit, RunnerError> {
    let _waiting = CustodyWait::enter();
    if let Some((code, workload)) = super::pending_stop(&run.target().label) {
        return Ok(PlainExit {
            code,
            workload,
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
    }
    let spec = ProcessSpec {
        argv: vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(command.command),
        ],
        cwd: command.cwd.to_path_buf(),
        env: environment(command.cwd, command.env)?,
        grace: GRACE,
        lifetime: match (
            command.timeout_ms.map(Duration::from_millis),
            super::spawn_env::remaining(),
        ) {
            (Some(local), Some(parent)) => Lifetime::Finite(local.min(parent)),
            (Some(limit), None) | (None, Some(limit)) => Lifetime::Finite(limit),
            (None, None) => Lifetime::Open(OpenReason::Shell),
        },
        terminal: Terminal::Inherit {
            capture: command.capture,
        },
        label: run.target().label.clone(),
    };
    let cx = kernel::effects();
    let mut process = Custody::spawn(&mut run.run_mut(), spec, &cx).map_err(|error| {
        RunnerError::TaskCommandLaunch {
            command: command.command.to_owned(),
            error: std::io::Error::other(format!("{error:?}")),
        }
    })?;
    let mut stdout = Tail::default();
    let mut stderr = Tail::default();
    let mut signalled: Option<(i32, Instant)> = None;
    let mut timed_out = false;
    let mut requested = false;
    let exit = loop {
        if process.expired() {
            timed_out = true;
            break None;
        }
        if super::run::stop_requested() {
            requested = true;
            break None;
        }
        for routed in kernel::poll_router() {
            let _ = process.forward(routed.forward);
            if signalled.is_none() {
                signalled = Some((routed.received, Instant::now()));
            }
        }
        match process.poll() {
            Observed::Verified(exit, _) => break Some(exit),
            _ if signalled.is_some_and(|(_, at)| at.elapsed() >= SIGNAL_EXIT_BUDGET) => {
                // Settle escalates TERM to KILL and verifies the group.
                break None;
            }
            _ => {}
        }
        if command.capture {
            let (out, err) = process.take_stream_output();
            stdout.push(&out);
            stderr.push(&err);
        }
        std::thread::sleep(POLL);
    };
    if command.capture {
        let (out, err) = process.take_stream_output();
        stdout.push(&out);
        stderr.push(&err);
    }
    let code = exit.map(|exit| exit.code);
    if let Some(exit) = exit {
        // Natural completion is recorded only from the verified exit (R5).
        let _ = process.complete(&mut run.run_mut(), exit);
    }
    let workload = if timed_out {
        WorkloadOutcome::TimedOut {
            phase: run.target().label.clone(),
        }
    } else if requested {
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        }
    } else {
        workload(code, signalled.map(|(signal, _)| signal))
    };
    let code = match (&workload, code) {
        (
            WorkloadOutcome::Cancelled {
                signal: Some(signal),
                ..
            },
            _,
        ) => 128 + signal,
        (WorkloadOutcome::TimedOut { .. }, _) => 124,
        (_, Some(code)) => code,
        (_, None) => 1,
    };
    Ok(PlainExit {
        workload,
        code,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

/// Runs env-schema `exec()` values as effects of the task's own run: each
/// command is journalled before it starts (K3), bounded by its timeout and
/// its capture limits (K8), and its group is stopped by the run's settle.
pub(in crate::runner) struct RunExec<'r> {
    run: std::cell::RefCell<&'r mut TaskRun>,
}

impl<'r> RunExec<'r> {
    pub(in crate::runner) fn new(run: &'r mut TaskRun) -> Self {
        Self {
            run: std::cell::RefCell::new(run),
        }
    }
}

impl effigy_env::ExecRunner for RunExec<'_> {
    fn run(
        &self,
        command: &str,
        timeout: Duration,
        cwd: &Path,
    ) -> Result<String, effigy_env::EnvSchemaError> {
        let spawn_error = |error: String| effigy_env::EnvSchemaError::ExecSpawn {
            command: command.to_owned(),
            error: std::io::Error::other(error),
        };
        let mut run = self.run.borrow_mut();
        let spec = ProcessSpec {
            argv: vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from(command),
            ],
            cwd: cwd.to_path_buf(),
            env: environment(cwd, Vec::new()).map_err(|error| spawn_error(error.to_string()))?,
            grace: Duration::from_secs(1),
            lifetime: Lifetime::Finite(timeout),
            terminal: Terminal::None,
            label: "env-schema exec".to_owned(),
        };
        let _waiting = CustodyWait::enter();
        let cx = kernel::effects();
        let mut process = Custody::spawn(&mut run.run_mut(), spec, &cx)
            .map_err(|error| spawn_error(format!("{error:?}")))?;
        let mut stdout = Vec::new();
        let mut stderr = Tail::default();
        let exit = loop {
            if !kernel::poll_router().is_empty() || process.expired() {
                break None;
            }
            let (out, err) = process.take_stream_output();
            stdout.extend_from_slice(&out);
            stderr.push(&err);
            if stdout.len() > effigy_env::STDOUT_MAX_BYTES {
                break None;
            }
            if let Observed::Verified(exit, _) = process.poll() {
                break Some(exit);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let (out, err) = process.take_stream_output();
        stdout.extend_from_slice(&out);
        stderr.push(&err);
        let Some(exit) = exit else {
            if let Ok(cleanup) = kernel::cleanup() {
                process.stop(&cleanup);
            }
            if stdout.len() > effigy_env::STDOUT_MAX_BYTES {
                return Err(effigy_env::EnvSchemaError::ExecFailed {
                    command: command.to_owned(),
                    code: None,
                    stderr: format!("stdout exceeded {} bytes", effigy_env::STDOUT_MAX_BYTES),
                });
            }
            return Err(effigy_env::EnvSchemaError::ExecTimeout {
                command: command.to_owned(),
                timeout,
            });
        };
        let _ = process.complete(&mut run.run_mut(), exit);
        if exit.code != 0 {
            let tail = &stderr.bytes[stderr
                .bytes
                .len()
                .saturating_sub(effigy_env::STDERR_TAIL_BYTES)..];
            return Err(effigy_env::EnvSchemaError::ExecFailed {
                command: command.to_owned(),
                code: Some(exit.code),
                stderr: String::from_utf8_lossy(tail).trim().to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
    }
}

/// Exit 0 succeeds. A forwarded signal, or a group ended by INT, TERM or
/// HUP, is a cancellation with that signal; anything else is a failure.
pub(in crate::runner) fn workload(code: Option<i32>, forwarded: Option<i32>) -> WorkloadOutcome {
    if let Some(signal) = forwarded {
        return WorkloadOutcome::Cancelled {
            cause: StopCause::Signal(signal),
            signal: Some(signal),
        };
    }
    match code {
        Some(0) => WorkloadOutcome::Succeeded,
        Some(code) if [libc::SIGHUP, libc::SIGINT, libc::SIGTERM].contains(&(code - 128)) => {
            WorkloadOutcome::Cancelled {
                cause: StopCause::Signal(code - 128),
                signal: Some(code - 128),
            }
        }
        Some(code) => WorkloadOutcome::Failed { code },
        None => WorkloadOutcome::Interrupted,
    }
}

fn environment(
    cwd: &Path,
    overrides: Vec<(String, EnvOverride)>,
) -> Result<BTreeMap<OsString, EnvValue>, RunnerError> {
    let mut env: BTreeMap<OsString, EnvValue> = std::env::vars_os()
        // Each custody spawn carries its own run's nonce.
        .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect();
    let local_bin = cwd.join("node_modules/.bin");
    if local_bin.is_dir() {
        let path = match env.get(std::ffi::OsStr::new("PATH")) {
            Some(EnvValue::Plain(path)) if !path.is_empty() => {
                format!("{}:{}", local_bin.display(), path.to_string_lossy())
            }
            _ => local_bin.display().to_string(),
        };
        env.insert(
            OsString::from("PATH"),
            EnvValue::Plain(OsString::from(path)),
        );
    }
    for (key, value) in super::spawn_env::entries().into_iter().chain(overrides) {
        let value = match value {
            EnvOverride::Plain(value) => EnvValue::Plain(OsString::from(value)),
            EnvOverride::Secret(value) if value.is_empty() => EnvValue::Plain(OsString::new()),
            EnvOverride::Secret(value) => {
                EnvValue::Secret(SecretValue::new(value.into_bytes()).map_err(|error| {
                    RunnerError::task_invocation(format!("secret `{key}` is not passable: {error}"))
                })?)
            }
        };
        env.insert(OsString::from(key), value);
    }
    Ok(env)
}

/// A bounded capture that keeps the newest bytes (D1-23).
#[derive(Default)]
struct Tail {
    bytes: Vec<u8>,
}
impl Tail {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > CAPTURE_TAIL {
            let excess = self.bytes.len() - CAPTURE_TAIL;
            self.bytes.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_maps_exit_codes_and_signals() {
        assert_eq!(workload(Some(0), None), WorkloadOutcome::Succeeded);
        assert_eq!(workload(Some(2), None), WorkloadOutcome::Failed { code: 2 });
        assert_eq!(
            workload(Some(130), None),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Signal(libc::SIGINT),
                signal: Some(libc::SIGINT)
            }
        );
        assert_eq!(
            workload(Some(0), Some(libc::SIGTERM)),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Signal(libc::SIGTERM),
                signal: Some(libc::SIGTERM)
            }
        );
        assert_eq!(workload(None, None), WorkloadOutcome::Interrupted);
    }
}
