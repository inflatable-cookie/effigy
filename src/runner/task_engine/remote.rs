//! A container command under kernel remote custody (contract 052 §5.10).
//!
//! The run journals the remote intent before the exec client starts, the
//! workload counts as running only once its in-band registration is recorded,
//! and the run's settle observes and stops the registered session in the
//! container. A signal, stop request or timeout reaches the remote session,
//! never just the local client: client exit is never remote closure (K6).
//! The environment fence is the reconciler's current generation; the backend
//! is the one the environment record names (#232).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::environment::{ComposeRemote, ExecOptions};
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::ctx::{OpenReason, StopCause};
use effigy_kernel::custody::{EnvValue, Lifetime, SecretValue};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{RequiredObligation, WorkloadOutcome};
use effigy_kernel::remote::{
    ContainerTarget, RemoteCommand, RemoteCustody, RemoteError, RemoteExecAdapter, RemoteLaunch,
    RemoteTerminal, Signal,
};
use effigy_kernel::run::{Fence, RunError};

use super::kernel::{self, CustodyWait};
use super::process::{EnvOverride, PlainExit};
use super::run::TaskRun;
use crate::runner::error::RunnerError;

/// Registration must arrive within this budget of the exec client's start.
const ACK_TIMEOUT: Duration = Duration::from_secs(60);
/// Each backend call (identity, observation, signal) is bounded by this.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
/// TERM to KILL for the remote session at settle.
const GRACE: Duration = Duration::from_secs(3);
/// After a forwarded stop, how long the session gets to exit by itself.
const SIGNAL_EXIT_BUDGET: Duration = Duration::from_secs(10);
/// JSON output keeps this much of each stream, newest bytes last.
const CAPTURE_TAIL: usize = 8 << 20;
const POLL: Duration = Duration::from_millis(20);
/// A concurrent environment run between reading the fence and the intent:
/// the intent is refused cleanly and re-read, as an attach retries.
const FENCE_ATTEMPTS: usize = 3;

/// One container workload: what runs, where, and with which environment.
pub(in crate::runner) struct RemoteTask<'a> {
    /// The host directory the exec client runs in.
    pub(in crate::runner) repo_root: &'a Path,
    /// The verified full id of the service container.
    pub(in crate::runner) container_id: &'a str,
    pub(in crate::runner) argv: Vec<String>,
    /// The workload's environment inside the container. Secrets stay out
    /// of argv and are redacted from every capture (K12).
    pub(in crate::runner) env: Vec<(String, EnvOverride)>,
    pub(in crate::runner) user: Option<String>,
    pub(in crate::runner) workdir: Option<String>,
    pub(in crate::runner) capture: bool,
    pub(in crate::runner) timeout_ms: Option<u64>,
    pub(in crate::runner) label: String,
}

pub(in crate::runner) fn run_remote(
    run: &mut TaskRun,
    policy: &EffectiveContainerPolicy,
    task: RemoteTask<'_>,
) -> Result<PlainExit, RunnerError> {
    let selection = crate::runner::environment::backend(task.repo_root, policy);
    let mut adapter =
        ComposeRemote::<effigy_containers::environment::HostCli>::launcher(exec_options(&task));
    let target = adapter
        .resolve(&selection, task.container_id)
        .map_err(|error| launch_error(&task.label, &error))?
        .target();
    run_remote_with(run, task, &mut adapter, target, &mut || fence(policy))
}

/// How the exec client runs on the host for `task`.
pub(in crate::runner) fn exec_options(task: &RemoteTask<'_>) -> ExecOptions {
    let lifetime = match (
        task.timeout_ms.map(Duration::from_millis),
        super::spawn_env::remaining(),
    ) {
        (Some(local), Some(parent)) => Lifetime::Finite(local.min(parent)),
        (Some(limit), None) | (None, Some(limit)) => Lifetime::Finite(limit),
        (None, None) => Lifetime::Open(OpenReason::Shell),
    };
    ExecOptions {
        user: task.user.clone(),
        workdir: task.workdir.clone(),
        cwd: task.repo_root.to_path_buf(),
        client_env: client_env(),
        lifetime,
        label: task.label.clone(),
    }
}

/// [`run_remote`] with its adapter and exact target supplied.
pub(in crate::runner) fn run_remote_with(
    run: &mut TaskRun,
    task: RemoteTask<'_>,
    adapter: &mut dyn RemoteExecAdapter,
    target: ContainerTarget,
    fence: &mut dyn FnMut() -> Result<Fence, RunnerError>,
) -> Result<PlainExit, RunnerError> {
    let _waiting = CustodyWait::enter();
    if let Some((code, workload)) = super::pending_stop(&run.target().label) {
        return Ok(not_run(code, workload));
    }
    let terminal = terminal(task.capture);
    let deadline = task
        .timeout_ms
        .map(Duration::from_millis)
        .into_iter()
        .chain(super::spawn_env::remaining())
        .min()
        .map(|budget| Instant::now() + budget);
    let env = container_env(task.env)?;
    let cx = kernel::effects();
    let mut attempt = 0;
    let mut remote = loop {
        attempt += 1;
        let fence = fence()?;
        let launched = RemoteCustody::launch(
            &mut *run.run_mut()?,
            RemoteLaunch {
                env: fence,
                target: target.clone(),
                command: RemoteCommand {
                    argv: task.argv.iter().map(OsString::from).collect(),
                    env: env.clone(),
                    terminal,
                },
                ack_timeout: ACK_TIMEOUT,
                call_timeout: CALL_TIMEOUT,
                grace: GRACE,
            },
            &cx,
            adapter,
        );
        match launched {
            Ok(remote) => break remote,
            Err(RemoteError::Run(RunError::StaleGeneration { .. })) if attempt < FENCE_ATTEMPTS => {
                continue
            }
            Err(error) => {
                // Cancellation can win between intent and acknowledgement;
                // settlement still owns whatever was recorded.
                if let Some((code, workload)) = super::pending_stop(&run.target().label) {
                    return Ok(not_run(code, workload));
                }
                return Err(launch_error(&task.label, &remote_error(&error)));
            }
        }
    };
    let relay = !task.capture;
    let mut stdout = Tail::default();
    let mut stderr = Tail::default();
    let mut drain = |remote: &mut effigy_kernel::remote::OwnedRemote| {
        let (out, err) = remote.client().take_stream_output();
        let moved = !out.is_empty() || !err.is_empty();
        if relay {
            // Observability only: a closed reader never changes the run (K10).
            let _ = std::io::stdout()
                .lock()
                .write_all(&out)
                .and_then(|()| std::io::stdout().flush());
            let _ = std::io::stderr().lock().write_all(&err);
        } else {
            stdout.push(&out);
            stderr.push(&err);
        }
        moved
    };
    let mut stopping: Option<(Stop, Instant)> = None;
    let mut requests_read = Instant::now();
    let exit = loop {
        if stopping.is_none() {
            let expired = deadline.is_some_and(|at| Instant::now() >= at);
            let stop = if expired || remote.client().expired() {
                Some(Stop::TimedOut)
            } else if super::run::stop_requested() {
                Some(Stop::Requested)
            } else if let Some(routed) = kernel::poll_router().first() {
                Some(Stop::Signal(routed.received))
            } else if requests_read.elapsed() >= Duration::from_millis(250) {
                requests_read = Instant::now();
                own_stop_request(run).map(Stop::Request)
            } else {
                None
            };
            if let Some(stop) = stop {
                // The remote session gets TERM now; the run's settle escalates
                // and verifies closure, whatever the client does.
                if let Ok(cleanup) = kernel::cleanup() {
                    let _ = adapter.signal_session(&remote.registration, Signal::Term, &cleanup);
                }
                stopping = Some((stop, Instant::now()));
            }
        } else {
            let _ = kernel::poll_router();
        }
        match remote.client().poll() {
            Observed::Verified(exit, _) => break Some(exit),
            _ if stopping
                .as_ref()
                .is_some_and(|(_, at)| at.elapsed() >= SIGNAL_EXIT_BUDGET) =>
            {
                break None
            }
            _ => {}
        }
        if !drain(&mut remote) {
            std::thread::sleep(POLL);
        }
    };
    drain(&mut remote);
    let workload = match (&stopping, exit) {
        (Some((Stop::TimedOut, _)), _) => WorkloadOutcome::TimedOut {
            phase: run.target().label.clone(),
        },
        (Some((Stop::Requested, _)), _) => requested(),
        (Some((Stop::Request(request), _)), _) => {
            let _ = run.run_mut()?.resolve_request(request);
            requested()
        }
        (Some((Stop::Signal(signal), _)), _) => WorkloadOutcome::Cancelled {
            cause: StopCause::Signal(*signal),
            signal: Some(*signal),
        },
        (None, Some(exit)) if client_lost(adapter, &remote, exit.code) => {
            // The exec client died by a signal while the session runs on: the
            // workload has no outcome yet. Settle stops the session.
            let _ = remote.client().complete(&mut *run.run_mut()?, exit);
            WorkloadOutcome::Interrupted
        }
        (None, Some(exit)) => {
            // Natural completion: the backend reports the registered
            // leader's own exit. The session's closure is observed by settle.
            let _ = remote.client().complete(&mut *run.run_mut()?, exit);
            let _ = remote.complete(
                &mut *run.run_mut()?,
                RequiredObligation::ProcessExit(exit.code),
            );
            super::process::workload(Some(exit.code), None)
        }
        (None, None) => WorkloadOutcome::Interrupted,
    };
    let code = match (&workload, exit) {
        (
            WorkloadOutcome::Cancelled {
                signal: Some(signal),
                ..
            },
            _,
        ) => 128 + signal,
        (WorkloadOutcome::TimedOut { .. }, _) => 124,
        (
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
            _,
        ) => 1,
        (_, Some(exit)) => exit.code,
        (_, None) => 1,
    };
    let stopped = matches!(workload, WorkloadOutcome::Cancelled { .. });
    Ok(PlainExit {
        workload,
        code,
        stopped,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

/// Client exit is never remote closure (K6). A client that ended by a signal
/// while the registered session is not proven closed was lost, not finished:
/// its exit status is not the workload's.
fn client_lost(
    adapter: &mut dyn RemoteExecAdapter,
    remote: &effigy_kernel::remote::OwnedRemote,
    code: i32,
) -> bool {
    if code <= 128 {
        return false;
    }
    let cx = kernel::effects().operation().child(CALL_TIMEOUT);
    // Only a positive closure proof lets the client's status stand.
    !matches!(
        adapter.observe_session(&remote.registration, &cx),
        Observed::Verified(effigy_kernel::remote::SessionState::Closed, _) | Observed::Absent(_)
    )
}

enum Stop {
    TimedOut,
    Requested,
    Request(effigy_kernel::run::Request),
    Signal(i32),
}

fn requested() -> WorkloadOutcome {
    WorkloadOutcome::Cancelled {
        cause: StopCause::Requested,
        signal: None,
    }
}

fn own_stop_request(run: &mut TaskRun) -> Option<effigy_kernel::run::Request> {
    run.run_mut()
        .ok()?
        .requests()
        .ok()?
        .into_iter()
        .find(|request| matches!(request.action, effigy_kernel::run::RequestAction::Stop))
}

fn not_run(code: i32, workload: WorkloadOutcome) -> PlainExit {
    let stopped = matches!(workload, WorkloadOutcome::Cancelled { .. });
    PlainExit {
        code,
        workload,
        stopped,
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}

/// Parity with v0.14.1: the workload reads the operator's stdin, and gets a
/// terminal when the operator has one and the output is not captured.
fn terminal(capture: bool) -> RemoteTerminal {
    if !capture
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
    {
        RemoteTerminal::Tty
    } else {
        RemoteTerminal::Stdin
    }
}

/// Whether a workload with this capture mode gets a backend terminal.
pub(in crate::runner) fn remote_tty(capture: bool) -> bool {
    terminal(capture) == RemoteTerminal::Tty
}

/// The environment's current generation from its record.
fn fence(policy: &EffectiveContainerPolicy) -> Result<Fence, RunnerError> {
    let record = crate::runner::environment::record(policy).ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "environment `{}` has no record yet: run `effigy container up --detach`, then retry",
            policy.name
        ))
    })?;
    Ok(Fence {
        resource: record.resource,
        generation: record.generation,
    })
}

/// The exec client's own environment: the host's, minus any custody nonce.
fn client_env() -> BTreeMap<OsString, EnvValue> {
    std::env::vars_os()
        .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
        .map(|(key, value)| (key, EnvValue::Plain(value)))
        .collect()
}

/// The guest workload's environment from this run's spawned entries.
///
/// A container workload is a workload: the same rule the host spawn helper
/// applies holds here, so no entry - ambient, task env or declared secret -
/// hands session vault access to the guest (contract 032; effigy#338,
/// effigy#445).
fn container_env(
    entries: Vec<(String, EnvOverride)>,
) -> Result<BTreeMap<OsString, EnvValue>, RunnerError> {
    let mut env = BTreeMap::new();
    for (key, value) in super::spawn_env::entries().into_iter().chain(entries) {
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
    env.retain(|key, _| !crate::runner::secret_session::is_session_access_key(key));
    Ok(env)
}

fn remote_error(error: &RemoteError) -> String {
    match error {
        RemoteError::Unknown(reason) => format!("{reason:?}"),
        RemoteError::Run(error) => format!("{error:?}"),
        RemoteError::Spawn(error) => format!("{error:?}"),
    }
}

fn launch_error(label: &str, error: &str) -> RunnerError {
    RunnerError::TaskCommandLaunch {
        command: format!("{label} (container)"),
        error: std::io::Error::other(error.to_owned()),
    }
}

/// A bounded capture that keeps the newest bytes.
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
mod tests;
