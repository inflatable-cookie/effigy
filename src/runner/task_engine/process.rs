//! A plain task command under kernel custody (contract 052 §5.7, K6, K9).
//!
//! The command always gets its own process group behind the spawn barrier,
//! with its class-appropriate custody receipt before it executes. Signals reach this
//! process through the one router and are forwarded to that group only. The
//! run's settle verifies the group closed, so nothing the task started
//! outlives the run. Text and JSON differ only in how captured output is
//! rendered (K11): text relays it as it arrives, JSON keeps a tail.
//!
//! A run named by `--run-id` captures its output, and so logs it for
//! `effigy run logs`, unless both stdout and stderr are terminals; this
//! process relays it as it arrives. Other text runs keep inheriting the
//! operator's descriptors untouched. A `run stop` request reaches the owner
//! through the journal and stops the task's group as a signal would.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{OpenReason, StopCause};
use effigy_kernel::custody::{EnvValue, Lifetime, ProcessSpec, SecretValue, Terminal};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::run::{Request, RequestAction};

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
/// The journal label of an env-schema `exec()` command: part of the run,
/// never its workload output.
pub(super) const ENV_EXEC_LABEL: &str = "env-schema exec";
/// How often a waiting owner reads its run's pending stop requests.
const REQUEST_POLL: Duration = Duration::from_millis(250);

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
    /// A stop of this run ended this workload: it never reported a result of
    /// its own. Recorded per workload, never inferred from an exit status.
    pub(in crate::runner) stopped: bool,
    pub(in crate::runner) stdout: Vec<u8>,
    pub(in crate::runner) stderr: Vec<u8>,
}

pub(in crate::runner) fn run_plain(
    run: &mut TaskRun,
    command: PlainCommand<'_>,
) -> Result<PlainExit, RunnerError> {
    run_plain_inner(run, command, false)
}

/// A plain command that is Effigy's own child process (a builtin or a script
/// host). It receives this session's vault access through the one-shot
/// descriptor handoff, so a nested Effigy step resolves without a prompt and
/// without the passphrase in its environment (effigy#338).
pub(in crate::runner) fn run_effigy_child(
    run: &mut TaskRun,
    command: PlainCommand<'_>,
) -> Result<PlainExit, RunnerError> {
    run_plain_inner(run, command, true)
}

fn run_plain_inner(
    run: &mut TaskRun,
    command: PlainCommand<'_>,
    effigy_child: bool,
) -> Result<PlainExit, RunnerError> {
    let _waiting = CustodyWait::enter();
    if let Some((code, workload)) = super::pending_stop(&run.target().label) {
        return Ok(PlainExit {
            code,
            workload,
            stopped: true,
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
    }
    let relay = !command.capture
        && run.keyed()
        && !(std::io::stdout().is_terminal() && std::io::stderr().is_terminal());
    let mut env = environment(command.cwd, command.env)?;
    let grant = if effigy_child {
        crate::runner::secret_session::take_child_grant()
    } else {
        None
    };
    if let Some(grant) = &grant {
        let (key, value) = grant.env_entry();
        env.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
    }
    let spec = ProcessSpec {
        argv: vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(command.command),
        ],
        cwd: command.cwd.to_path_buf(),
        env,
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
            capture: command.capture || relay,
        },
        label: run.target().label.clone(),
    };
    let spawned = run.spawn_with_handoff(spec, false, grant.map(|grant| grant.into_reader()));
    let mut process = match spawned {
        Ok(process) => process,
        Err(error) => {
            // Cancellation can win after the spawn barrier releases workload,
            // before spawn returns. Settlement still owns that recorded effect.
            if let Some((code, workload)) = super::pending_stop(&run.target().label) {
                return Ok(PlainExit {
                    code,
                    workload,
                    stopped: true,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                });
            }
            // An admission refusal keeps its own error shape: the workload
            // was stopped before `go` because its report was not
            // acknowledged, and the caller must see `effigy.admission.v1`.
            return match error {
                refusal @ RunnerError::AdmissionRefused { .. } => Err(refusal),
                other => Err(RunnerError::TaskCommandLaunch {
                    command: command.command.to_owned(),
                    error: std::io::Error::other(format!("{other:?}")),
                }),
            };
        }
    };
    let mut stdout = Tail::default();
    let mut stderr = Tail::default();
    let mut drain = |process: &effigy_kernel::custody::OwnedProcess| {
        let (out, err) = process.take_stream_output();
        let moved = !out.is_empty() || !err.is_empty();
        if relay {
            // Observability only: a closed reader never changes the run (K10).
            let _ = std::io::stdout()
                .lock()
                .write_all(&out)
                .and_then(|()| std::io::stdout().flush());
            let _ = std::io::stderr().lock().write_all(&err);
        } else if command.capture {
            stdout.push(&out);
            stderr.push(&err);
        }
        moved
    };
    let mut signalled: Option<(i32, Instant)> = None;
    let mut timed_out = false;
    let mut requested = false;
    // A `run stop` addressed to this run outside any parent scope: TERM to
    // the group, then the run records `cancelled` by request, exactly as the
    // parent-scope path above does.
    let mut own_request: Option<(Request, Instant)> = None;
    let mut requests_read = Instant::now();
    let exit = loop {
        if let Err(error) = run.verify_admission() {
            if let Ok(cleanup) = kernel::cleanup() {
                process.stop(&cleanup);
            }
            return Err(error);
        }
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
        if own_request.is_none() && requests_read.elapsed() >= REQUEST_POLL {
            requests_read = Instant::now();
            if let Some(request) = stop_request(run) {
                let _ = process.forward(libc::SIGTERM);
                own_request = Some((request, Instant::now()));
            }
        }
        match process.poll() {
            Observed::Verified(exit, _) => break Some(exit),
            _ if signalled
                .map(|(_, at)| at)
                .or(own_request.as_ref().map(|(_, at)| *at))
                .is_some_and(|at| at.elapsed() >= SIGNAL_EXIT_BUDGET) =>
            {
                // Settle escalates TERM to KILL and verifies the group.
                break None;
            }
            _ => {}
        }
        if !drain(&process) {
            std::thread::sleep(POLL);
        }
    };
    drain(&process);
    let code = exit.map(|exit| exit.code);
    if let Some(exit) = exit {
        // Natural completion is recorded only from the verified exit (R5).
        let _ = run.complete(&process, exit);
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
    } else if let Some((request, _)) = own_request.as_ref().filter(|_| signalled.is_none()) {
        let _ = run.run_mut()?.resolve_request(request);
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
        // Cancelled by request: the group's own exit (often 143) is not the
        // run's result.
        (
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
            _,
        ) => 1,
        (_, Some(code)) => code,
        (_, None) => 1,
    };
    Ok(PlainExit {
        workload,
        code,
        stopped: requested || own_request.is_some() || signalled.is_some(),
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
            label: ENV_EXEC_LABEL.to_owned(),
        };
        let _waiting = CustodyWait::enter();
        let mut process = run
            .spawn(spec, false)
            .map_err(|error| spawn_error(format!("{error:?}")))?;
        let mut stdout = Vec::new();
        let mut stderr = Tail::default();
        let exit = loop {
            if let Err(error) = run.verify_admission() {
                if let Ok(cleanup) = kernel::cleanup() {
                    process.stop(&cleanup);
                }
                return Err(spawn_error(error.to_string()));
            }
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
        let _ = run.complete(&process, exit);
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

/// A pending `run stop` for this run, read from the journal.
fn stop_request(run: &mut TaskRun) -> Option<Request> {
    run.run_mut()
        .ok()?
        .requests()
        .ok()?
        .into_iter()
        .find(|request| matches!(request.action, RequestAction::Stop))
}

/// The spawn environment a run gives a workload in `cwd`, before its own
/// overrides: this process's environment without custody nonces.
pub(in crate::runner) fn spawn_environment(
    cwd: &Path,
) -> Result<BTreeMap<OsString, EnvValue>, RunnerError> {
    environment(cwd, Vec::new())
}

pub(in crate::runner) fn environment(
    cwd: &Path,
    overrides: Vec<(String, EnvOverride)>,
) -> Result<BTreeMap<OsString, EnvValue>, RunnerError> {
    // A workload's ambient environment never carries session vault access:
    // the passphrase, its local-dev marker and the caller's source stay with
    // Effigy's own processes (effigy#338, K12).
    let mut env: BTreeMap<OsString, EnvValue> =
        crate::runner::secret_session::workload_ambient_environment()
            // Each custody spawn carries its own run's nonce.
            .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
            .map(|(key, value)| (key, EnvValue::Plain(value)))
            .collect();
    // A child of an admitted run never queues for admission again.
    if let Some(marker) = super::admission::child_marker() {
        env.insert(
            OsString::from(super::admission::ADMITTED_ENV),
            EnvValue::Plain(OsString::from(marker)),
        );
    }
    let local_bin = cwd.join("node_modules/.bin");
    // A program fallback's shim is first: it shadows the program this run named,
    // and a task that sets `PATH` explicitly still replaces both prefixes.
    let mut prefixes: Vec<std::path::PathBuf> = Vec::new();
    if let Some(shims) = crate::runner::program_fallback::path_dir() {
        prefixes.push(shims);
    }
    if local_bin.is_dir() {
        prefixes.push(local_bin);
    }
    if !prefixes.is_empty() {
        let inherited = match env.get(std::ffi::OsStr::new("PATH")) {
            Some(EnvValue::Plain(path)) if !path.is_empty() => Some(path.clone()),
            _ => None,
        };
        let mut path = OsString::new();
        for (index, prefix) in prefixes.iter().enumerate() {
            if index > 0 {
                path.push(":");
            }
            path.push(prefix.as_os_str());
        }
        if let Some(inherited) = inherited {
            path.push(":");
            path.push(inherited);
        }
        env.insert(OsString::from("PATH"), EnvValue::Plain(path));
    }
    // The run's own scratch directory is the only temp directory a host
    // process sees: set only when this run owns it (neither an explicit nor a
    // non-default `TMPDIR` was inherited). Task env and env-schema values are
    // applied after, so an explicit override still wins.
    for (key, value) in crate::runner::scratch::temp_env() {
        env.insert(OsString::from(key), EnvValue::Plain(value.into_os_string()));
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
    // A declared override cannot hand session access back to a workload
    // either: the route is Effigy's own descriptor, not a name.
    env.retain(|key, _| !crate::runner::secret_session::is_session_access_key(key));
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
