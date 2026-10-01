//! Submit one heavy invocation to the scheduler, relay its output, follow
//! cancellation, and translate the settlement into an honest process status.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use effigy_host_run::{
    new_client_request_id, AttachEvent, BudgetFallback, ClassSource, ClientError, HostRunClient,
    HostRunRoot, OutputStream, Priority, RunClass, Settlement, SettlementOutcome, SubmitRequest,
};
use serde_json::Value;

use super::{open_client, refuse, RUN_ID_ENV, TOKEN_ENV};
use crate::runner::error::RunnerError;

const DEFAULT_RUN_TIMEOUT_SECS: u64 = 2 * 60 * 60;
const RUN_TIMEOUT_ENV: &str = "EFFIGY_HOST_SCHEDULER_RUN_TIMEOUT_SECS";
const LEASE_ENV: &str = "EFFIGY_ADMISSION_LEASE_ID";

/// What the caller resolved before submitting.
pub(in crate::runner) struct SubmitContext<'a> {
    pub(in crate::runner) selector: &'a str,
    pub(in crate::runner) class_source: ClassSource,
    pub(in crate::runner) repository: &'a Path,
    pub(in crate::runner) cwd: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum PreLaunch {
    CapacityTimeout,
    Cancelled,
}

/// How a submitted run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum Settled {
    /// The scheduler launched the child. `exit_code` is its real status; its
    /// output was already relayed.
    Launched {
        run_id: String,
        epoch: u64,
        exit_code: i32,
        outcome: SettlementOutcome,
        queue_wait_ms: Option<u64>,
    },
    /// The run settled without ever launching.
    NotLaunched {
        run_id: String,
        epoch: u64,
        reason: PreLaunch,
        queue_wait_ms: Option<u64>,
        interrupt_signal: Option<i32>,
    },
}

impl Settled {
    /// The error a caller returns for this outcome: the child's real status
    /// when it launched, a distinct typed refusal when it did not.
    pub(in crate::runner) fn into_error(self) -> RunnerError {
        match self {
            Self::Launched { exit_code, .. } => RunnerError::HostRunSettled { code: exit_code },
            Self::NotLaunched {
                run_id,
                reason,
                interrupt_signal,
                ..
            } => match reason {
                PreLaunch::CapacityTimeout => refuse(
                    1,
                    format!(
                        "capacity_timeout: heavy run {run_id} was never launched because capacity did not free in time; this is a capacity timeout, not a validation failure"
                    ),
                ),
                PreLaunch::Cancelled => refuse(
                    interrupt_signal.map_or(1, |signal| 128 + signal),
                    format!("cancelled: heavy run {run_id} was cancelled before launch"),
                ),
            },
        }
    }
}

#[derive(Default)]
struct InterruptState {
    run_id: Option<String>,
    signal: Option<i32>,
    cancel_sent: bool,
}

/// Turns the first SIGINT/SIGTERM/SIGHUP into one scheduler cancel for this
/// run. The cancel is sent by whichever side learns the second fact (run id or
/// signal) last, so an interrupt during submit is not lost.
struct Interrupt {
    state: Mutex<InterruptState>,
    root: PathBuf,
}

impl Interrupt {
    fn signal(&self) -> Option<i32> {
        self.lock().signal
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InterruptState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_run_id(&self, run_id: &str) {
        self.lock().run_id = Some(run_id.to_owned());
        self.cancel_if_ready();
    }

    fn record_signal(&self, signal: i32) {
        {
            let mut state = self.lock();
            if state.signal.is_none() {
                state.signal = Some(signal);
            }
        }
        self.cancel_if_ready();
    }

    fn cancel_if_ready(&self) {
        let (run_id, signal) = {
            let mut state = self.lock();
            let (Some(run_id), Some(signal)) = (state.run_id.clone(), state.signal) else {
                return;
            };
            if state.cancel_sent {
                return;
            }
            state.cancel_sent = true;
            (run_id, signal)
        };
        let reason = format!("client interrupted by {}", signal_name(signal));
        let sent = HostRunRoot::open(&self.root)
            .map_err(|error| error.to_string())
            .and_then(|(root, authority)| {
                HostRunClient::open(root, authority)
                    .cancel(&run_id, &reason)
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = sent {
            eprintln!("warning: could not ask the scheduler to cancel run {run_id}: {error}");
        }
    }
}

struct InterruptListener {
    handle: signal_hook::iterator::Handle,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl InterruptListener {
    fn install(interrupt: Arc<Interrupt>) -> std::io::Result<Self> {
        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        let mut signals = signal_hook::iterator::Signals::new([SIGHUP, SIGINT, SIGTERM])?;
        let handle = signals.handle();
        let thread = std::thread::Builder::new()
            .name("effigy-host-run-interrupt".to_owned())
            .spawn(move || {
                for signal in signals.forever() {
                    interrupt.record_signal(signal);
                }
            })?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}

impl Drop for InterruptListener {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Submit this invocation's exact argv, cwd and environment, stream its
/// output, and return how it ended. Nothing launches locally.
pub(in crate::runner) fn submit_and_settle(ctx: SubmitContext<'_>) -> Result<Settled, RunnerError> {
    let (mut client, root) = open_client()?;
    let request = build_request(&ctx)?;
    let interrupt = Arc::new(Interrupt {
        state: Mutex::new(InterruptState::default()),
        root: root.clone(),
    });
    let _listener = InterruptListener::install(Arc::clone(&interrupt)).map_err(|error| {
        refuse(
            1,
            format!("cannot install heavy-run interrupt handling: {error}"),
        )
    })?;

    let submitted_at = Instant::now();
    let submitted = client.submit_request(&request).map_err(|error| {
        let code = error.exit_code().map_or(1, i32::from);
        refuse(code, format!("host-run submit failed: {error}"))
    })?;
    let run_id = submitted.run_id;
    let epoch = client.authority().epoch;
    interrupt.set_run_id(&run_id);

    let mut relay = Relay::default();
    let settlement = client
        .attach_stream(&run_id, 0, 0, |event| {
            relay.handle(event, submitted_at);
        })
        .map_err(|error| attach_failure(&run_id, error))?;
    relay.finish();
    note_container_closure(&settlement);
    Ok(interpret(
        &settlement,
        epoch,
        relay.queue_wait_ms,
        elapsed_ms(submitted_at),
        interrupt.signal(),
    ))
}

fn build_request(ctx: &SubmitContext<'_>) -> Result<SubmitRequest, RunnerError> {
    let argv = invocation_argv()?;
    let (cpu_units, memory_mib) = crate::runner::admission::requested_reservation_units()
        .map_err(|error| refuse(2, error))?;
    let capacity_secs =
        crate::runner::admission::capacity_wait_secs().map_err(|error| refuse(2, error))?;
    let run_secs = match std::env::var(RUN_TIMEOUT_ENV) {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|secs| *secs > 0)
            .ok_or_else(|| refuse(2, format!("{RUN_TIMEOUT_ENV} must be a positive integer")))?,
        Err(_) => DEFAULT_RUN_TIMEOUT_SECS,
    };
    let mut env = forwarded_env();
    env.entry("CARGO_BUILD_JOBS".to_owned())
        .or_insert_with(|| cpu_units.to_string());
    let caller = std::env::var("EFFIGY_CALLER")
        .unwrap_or_else(|_| crate::runner::admission::default_caller_identity());
    Ok(SubmitRequest {
        client_request_id: new_client_request_id().map_err(|error| refuse(1, error.to_string()))?,
        caller,
        repository: path_string(ctx.repository)?,
        cwd: ctx.cwd.to_path_buf(),
        selector: ctx.selector.to_owned(),
        argv,
        class: RunClass::Heavy,
        class_source: ctx.class_source.clone(),
        priority: Priority::Validation,
        budget_fallback: BudgetFallback {
            cpu: cpu_units,
            memory_bytes: memory_mib.saturating_mul(1024 * 1024),
        },
        capacity_deadline_ms: capacity_secs.saturating_mul(1000).max(1),
        run_timeout_ms: run_secs.saturating_mul(1000),
        env,
        cancel_on_disconnect: false,
    })
}

fn path_string(path: &Path) -> Result<String, RunnerError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| refuse(2, "repository path is not valid UTF-8"))
}

/// The executable and arguments this process was started with. The scheduler
/// re-runs exactly this invocation, so selectors, targets and routing are the
/// ones the operator typed.
fn invocation_argv() -> Result<Vec<String>, RunnerError> {
    let exe = std::env::current_exe()
        .map_err(|error| refuse(1, format!("cannot resolve the effigy executable: {error}")))?;
    let mut argv = vec![path_string(&exe)?];
    for arg in std::env::args_os().skip(1) {
        let arg = arg.into_string().map_err(|_| {
            refuse(
                2,
                "an argument is not valid UTF-8 and cannot be submitted to the scheduler",
            )
        })?;
        argv.push(arg);
    }
    if argv.iter().any(String::is_empty) {
        return Err(refuse(
            2,
            "an empty argument cannot be submitted to the scheduler",
        ));
    }
    Ok(argv)
}

/// The caller's environment minus names the scheduler or legacy lease own. The
/// scheduler launches with only this map plus PATH, HOME and its own run
/// variables, so nothing else crosses.
fn forwarded_env() -> BTreeMap<String, String> {
    forward_env_from(std::env::vars_os())
}

pub(super) fn forward_env_from(
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> BTreeMap<String, String> {
    vars.filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(key, value)| {
            !key.is_empty()
                && !key.contains('=')
                && !key.contains('\0')
                && !value.contains('\0')
                && key != TOKEN_ENV
                && key != RUN_ID_ENV
                && key != LEASE_ENV
        })
        .collect()
}

#[derive(Default)]
struct Relay {
    last_position: Option<u64>,
    queue_wait_ms: Option<u64>,
    expired: Vec<&'static str>,
}

impl Relay {
    fn handle(&mut self, event: AttachEvent, submitted_at: Instant) {
        match event {
            AttachEvent::State { state, position } => {
                if state == "queued" {
                    if let Some(position) = position {
                        if self.last_position != Some(position) {
                            eprintln!("waiting for QA capacity, position {position}");
                            self.last_position = Some(position);
                        }
                    }
                } else if self.queue_wait_ms.is_none() {
                    self.queue_wait_ms = Some(elapsed_ms(submitted_at));
                }
            }
            AttachEvent::Output { stream, data, .. } => {
                if self.queue_wait_ms.is_none() {
                    self.queue_wait_ms = Some(elapsed_ms(submitted_at));
                }
                match stream {
                    OutputStream::Stdout => relay_bytes(&mut std::io::stdout().lock(), &data),
                    OutputStream::Stderr => relay_bytes(&mut std::io::stderr().lock(), &data),
                }
            }
            AttachEvent::OutputExpired {
                stream,
                available_from,
            } => {
                let name = match stream {
                    OutputStream::Stdout => "stdout",
                    OutputStream::Stderr => "stderr",
                };
                self.expired.push(name);
                eprintln!(
                    "output_expired: {name} before byte {available_from} is no longer retained by the scheduler; relayed output is incomplete"
                );
            }
            AttachEvent::Settled(_) => {}
        }
    }

    fn finish(&mut self) {
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
    }
}

fn relay_bytes(out: &mut impl Write, data: &[u8]) {
    // A closed pipe must not turn a finished child into a client failure.
    let _ = out.write_all(data);
    let _ = out.flush();
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn attach_failure(run_id: &str, error: ClientError) -> RunnerError {
    let code = error.exit_code().map_or(70, i32::from);
    refuse(
        code,
        format!(
            "lost contact with the scheduler while following run {run_id}: {error}. The run's final state is unknown; inspect it with the host-run status for this run id"
        ),
    )
}

/// Container closure the scheduler reports is its own host-group evidence; a
/// removal that is not confirmed is surfaced, never rounded up to success.
fn note_container_closure(settlement: &Settlement) {
    for container in &settlement.containers {
        let removed = container.get("removed");
        if removed != Some(&Value::Bool(true)) {
            eprintln!(
                "warning: container {} ({}) removal is {}; host-group closure does not prove container closure",
                container["id"].as_str().unwrap_or("?"),
                container["runtime"].as_str().unwrap_or("?"),
                match removed {
                    Some(Value::Bool(false)) => "false",
                    _ => "unknown",
                }
            );
        }
    }
}

/// Translate a validated settlement. Pure so every outcome has a unit test.
pub(super) fn interpret(
    settlement: &Settlement,
    epoch: u64,
    observed_queue_wait_ms: Option<u64>,
    total_wait_ms: u64,
    interrupt_signal: Option<i32>,
) -> Settled {
    let run_id = settlement.run_id.clone();
    if !settlement.launched {
        let reason = match settlement.outcome {
            SettlementOutcome::CapacityTimeout => PreLaunch::CapacityTimeout,
            _ => PreLaunch::Cancelled,
        };
        return Settled::NotLaunched {
            run_id,
            epoch,
            reason,
            // Never launched: the whole wait after submit was queue wait.
            queue_wait_ms: Some(total_wait_ms),
            interrupt_signal,
        };
    }
    let exit_code = launched_exit_code(settlement);
    match settlement.outcome {
        SettlementOutcome::TimedOut => eprintln!(
            "run_timeout: the scheduler stopped run {} after its run deadline",
            settlement.run_id
        ),
        SettlementOutcome::Lost => eprintln!(
            "lost: the scheduler lost run {}; its result is unknown",
            settlement.run_id
        ),
        SettlementOutcome::Cancelled => {
            eprintln!(
                "cancelled: run {} was cancelled after launch",
                settlement.run_id
            )
        }
        _ => {}
    }
    Settled::Launched {
        run_id,
        epoch,
        exit_code,
        outcome: settlement.outcome,
        queue_wait_ms: observed_queue_wait_ms,
    }
}

/// The child's real status; a non-pass outcome never reports 0.
pub(super) fn launched_exit_code(settlement: &Settlement) -> i32 {
    let from_result = settlement.result.as_ref().and_then(|result| {
        if let Some(code) = result.get("exitCode").and_then(Value::as_i64) {
            return i32::try_from(code).ok();
        }
        result
            .get("signal")
            .map(|signal| 128 + signal_number(signal).unwrap_or(0))
    });
    match settlement.outcome {
        SettlementOutcome::TimedOut => 124,
        SettlementOutcome::Lost => 70,
        SettlementOutcome::Passed => from_result.unwrap_or(0),
        _ => match from_result {
            Some(0) | None => 1,
            Some(code) => code,
        },
    }
}

fn signal_number(value: &Value) -> Option<i32> {
    if let Some(number) = value.as_u64() {
        return i32::try_from(number).ok();
    }
    Some(match value.as_str()? {
        "SIGHUP" => 1,
        "SIGINT" => 2,
        "SIGQUIT" => 3,
        "SIGABRT" => 6,
        "SIGKILL" => 9,
        "SIGSEGV" => 11,
        "SIGPIPE" => 13,
        "SIGALRM" => 14,
        "SIGTERM" => 15,
        _ => return None,
    })
}

fn signal_name(signal: i32) -> &'static str {
    match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        15 => "SIGTERM",
        _ => "a signal",
    }
}
