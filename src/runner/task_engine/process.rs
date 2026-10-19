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

pub(in crate::runner) struct PlainCommand<'a> {
    pub(in crate::runner) command: &'a str,
    pub(in crate::runner) cwd: &'a Path,
    /// Overrides on top of this process's environment. Secrets stay out of
    /// argv and are redacted from any capture (K12).
    pub(in crate::runner) env: Vec<(String, EnvOverride)>,
    pub(in crate::runner) capture: bool,
}

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
    pub(in crate::runner) truncated: bool,
}

pub(in crate::runner) fn run_plain(
    run: &mut TaskRun,
    command: PlainCommand<'_>,
) -> Result<PlainExit, RunnerError> {
    let spec = ProcessSpec {
        argv: vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(command.command),
        ],
        cwd: command.cwd.to_path_buf(),
        env: environment(command.cwd, command.env)?,
        grace: GRACE,
        lifetime: Lifetime::Open(OpenReason::Shell),
        terminal: Terminal::Inherit {
            capture: command.capture,
        },
        label: run.target().label.clone(),
    };
    let _waiting = CustodyWait::enter();
    let cx = kernel::effects();
    let mut process = Custody::spawn(run.run_mut(), spec, &cx).map_err(|error| {
        RunnerError::TaskCommandLaunch {
            command: command.command.to_owned(),
            error: std::io::Error::other(format!("{error:?}")),
        }
    })?;
    let mut stdout = Tail::default();
    let mut stderr = Tail::default();
    let mut signalled: Option<(i32, Instant)> = None;
    let exit = loop {
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
        std::thread::sleep(Duration::from_millis(5));
    };
    if command.capture {
        let (out, err) = process.take_stream_output();
        stdout.push(&out);
        stderr.push(&err);
    }
    let code = exit.map(|exit| exit.code);
    if let Some(exit) = exit {
        // Natural completion is recorded only from the verified exit (R5).
        let _ = process.complete(run.run_mut(), exit);
    }
    let workload = workload(code, signalled.map(|(signal, _)| signal));
    let code = match (&workload, code) {
        (WorkloadOutcome::Cancelled { signal: Some(signal), .. }, _) => 128 + signal,
        (_, Some(code)) => code,
        (_, None) => 1,
    };
    Ok(PlainExit {
        workload,
        code,
        truncated: stdout.truncated || stderr.truncated,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
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
        env.insert(OsString::from("PATH"), EnvValue::Plain(OsString::from(path)));
    }
    for (key, value) in overrides {
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

#[derive(Default)]
struct Tail {
    bytes: Vec<u8>,
    truncated: bool,
}
impl Tail {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > CAPTURE_TAIL {
            let excess = self.bytes.len() - CAPTURE_TAIL;
            self.bytes.drain(..excess);
            self.truncated = true;
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
