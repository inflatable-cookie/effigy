//! The client of a managed run: it starts the run's detached supervisor
//! and is one of its front ends (§5.8 steps 1, 2 and 6).
//!
//! The text front end follows the run's own output and waits for it to
//! close; the detached front end returns once the run is active; the TUI
//! renders every workload. Each is the run's owning client until it
//! detaches: its death, by terminal close, SIGHUP or an agent's SIGKILL,
//! stops and settles the run. INT, TERM and HUP to a client send the stop
//! itself and wait for the settle; a second interrupt within two seconds
//! escalates to KILL.

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{Deadline, EffectClass, OpenReason};
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::RunError;
use effigy_kernel::supervise::{
    start_engine, ClientMode, EngineLaunch, Event, SupervisorClient, OUTPUT_SEQUENCE,
};

use super::control::{self, Addressed};
use super::wire::{self, EngineEvent, Front, Payload};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, read_run_log};

/// How long a client waits for one supervisor event before its next turn.
const TURN: Duration = Duration::from_millis(50);
const READ_MAX: usize = 64 * 1024;

/// What the client knows when it starts a run's supervisor.
pub(in crate::runner) struct ClientStart<'a> {
    pub(in crate::runner) addressed: Addressed<'a>,
    pub(in crate::runner) invocation: &'a effigy_cli::TaskInvocation,
    pub(in crate::runner) cwd: PathBuf,
    pub(in crate::runner) front: Front,
    pub(in crate::runner) terminal: Option<(u16, u16)>,
}

/// A started run: its owning client, until it detaches or the run closes.
pub(in crate::runner) struct Started {
    pub(in crate::runner) client: SupervisorClient,
    pub(in crate::runner) run: RunId,
}

/// Spawns the run's detached supervisor and waits for its admission. A
/// refusal (the run is already live, its predecessor would not settle, a
/// setup error) is reported here; nothing was started.
pub(in crate::runner) fn start(request: &ClientStart<'_>) -> Result<Started, RunnerError> {
    super::legacy::refuse_live_session(&request.addressed)?;
    let home = task_engine::state_home_path().ok_or_else(|| {
        RunnerError::task_invocation(
            "HOME is unset or relative; Effigy keeps run state under $HOME/.effigy/state",
        )
    })?;
    let payload = Payload {
        task: request.invocation.name.clone(),
        args: request.invocation.args.clone(),
        front: request.front,
        state_home: home.clone(),
        terminal: request.terminal,
    };
    let mut env: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
        .collect();
    // The vault passphrase this client already read travels in the spawn
    // environment only (K12), so the supervisor never prompts.
    if let Some(passphrase) = crate::runner::secret_session::session_passphrase() {
        let key = OsString::from(crate::runner::secret_session::internal_secret_passphrase_env());
        env.retain(|(existing, _)| existing != &key);
        env.push((key, OsString::from(passphrase.expose())));
    }
    let executable = task_engine::startup_executable()?;
    let cx = task_engine::effect_context(OpenReason::DevServer);
    let client = start_engine(
        &cx,
        &home,
        EngineLaunch {
            payload: wire::encode(&payload),
            cwd: request.cwd.clone(),
            env,
        },
        &executable,
        ClientMode::Owning,
    )
    .map_err(|error| refusal(&request.addressed, error))?;
    let run = client.run;
    Ok(Started { client, run })
}

fn refusal(addressed: &Addressed<'_>, error: RunError) -> RunnerError {
    match error {
        // The supervisor's own refusal, already a complete sentence.
        RunError::Invalid(reason) => RunnerError::task_invocation(reason),
        other => RunnerError::task_invocation(format!(
            "managed task `{}` (profile `{}`) did not start its supervisor: {other}",
            addressed.task, addressed.profile
        )),
    }
}

/// The run's own output, followed by byte offset (complete, unlike the
/// best-effort event pipe).
pub(in crate::runner) struct OutputFollower {
    run: RunId,
    offsets: [u64; 2],
}

impl OutputFollower {
    pub(in crate::runner) fn new(run: RunId) -> Self {
        Self {
            run,
            offsets: [0, 0],
        }
    }

    /// New output since the last read, stdout then stderr.
    pub(in crate::runner) fn read(&mut self) -> (Vec<u8>, Vec<u8>) {
        let mut out = (Vec::new(), Vec::new());
        for (slot, stream) in [Stream::Stdout, Stream::Stderr].into_iter().enumerate() {
            if let Observed::Verified(read, _) =
                read_run_log(self.run, OUTPUT_SEQUENCE, stream, self.offsets[slot], READ_MAX)
            {
                self.offsets[slot] = read.from + read.bytes.len() as u64;
                if slot == 0 {
                    out.0 = read.bytes;
                } else {
                    out.1 = read.bytes;
                }
            }
        }
        out
    }
}

/// How the client's view of the run ended.
pub(in crate::runner) struct Closed {
    pub(in crate::runner) error: Option<String>,
}

/// Stop requests a client sends on INT, TERM or HUP: the first stops, a
/// second within two seconds escalates.
pub(in crate::runner) struct StopSender {
    sent: Option<Instant>,
}

impl StopSender {
    pub(in crate::runner) fn new() -> Self {
        Self { sent: None }
    }

    pub(in crate::runner) fn requested(&self) -> bool {
        self.sent.is_some()
    }

    /// Sends a stop, or an escalation when one was already sent.
    pub(in crate::runner) fn send(&mut self, client: &SupervisorClient) {
        let Ok(request) = task_engine::fresh_request_id() else {
            return;
        };
        let cx = task_engine::effect_context(OpenReason::Shell);
        let _ = match self.sent {
            Some(at) if at.elapsed() <= Duration::from_secs(2) => client.escalate(&cx, request),
            _ => client.stop(&cx, request),
        };
        self.sent = Some(Instant::now());
    }
}

/// The attached text front end: the run's own output as it arrives, until
/// the run closes. Workload output stays in their logs (`<task> logs`).
pub(in crate::runner) fn attach_text(
    started: Started,
    addressed: &Addressed<'_>,
) -> Result<Option<String>, RunnerError> {
    task_engine::install_client_router()?;
    let Started { mut client, run } = started;
    println!(
        "[managed] `{}` (profile `{}`): run {run}; stop with `{}`, or interrupt this client",
        addressed.task,
        addressed.profile,
        stop_command(addressed)
    );
    flush();
    let mut output = OutputFollower::new(run);
    let mut stop = StopSender::new();
    let closed = loop {
        if !task_engine::routed_signals().is_empty() {
            stop.send(&client);
        }
        relay(&mut output);
        match next_event(&mut client) {
            Ok(Some(Event::Engine(bytes))) => {
                if let Some(EngineEvent::Outcome { error }) = wire::decode(&bytes) {
                    relay(&mut output);
                    // Closed follows the outcome on the same pipe.
                    wait_closed(&mut client);
                    relay(&mut output);
                    break Closed { error };
                }
            }
            Ok(Some(Event::Closed { .. })) => {
                relay(&mut output);
                break Closed { error: None };
            }
            Ok(_) => {}
            Err(lost) => {
                relay(&mut output);
                break supervisor_lost(run, addressed, &lost);
            }
        }
    };
    finish(addressed, closed)
}

/// The detached front end: returns once every workload has started, with
/// the run id and the commands that observe and stop it.
pub(in crate::runner) fn detach(
    started: Started,
    addressed: &Addressed<'_>,
) -> Result<Option<String>, RunnerError> {
    task_engine::install_client_router()?;
    let Started { mut client, run } = started;
    let mut output = OutputFollower::new(run);
    let mut stop = StopSender::new();
    loop {
        if !task_engine::routed_signals().is_empty() {
            // Interrupting a detaching client stops the run it started.
            stop.send(&client);
        }
        match next_event(&mut client) {
            Ok(Some(Event::Active)) if !stop.requested() => break,
            Ok(Some(Event::Engine(bytes))) => {
                if let Some(EngineEvent::Outcome { error }) = wire::decode(&bytes) {
                    wait_closed(&mut client);
                    relay(&mut output);
                    return finish(addressed, Closed { error });
                }
            }
            Ok(Some(Event::Closed { .. })) => {
                relay(&mut output);
                return finish(addressed, Closed { error: None });
            }
            Ok(_) => {}
            Err(lost) => {
                relay(&mut output);
                return finish(addressed, supervisor_lost(run, addressed, &lost));
            }
        }
    }
    let cx = task_engine::effect_context(OpenReason::Shell);
    let request = task_engine::fresh_request_id()?;
    client.detach(&cx, request).map_err(|error| {
        RunnerError::task_invocation(format!(
            "managed run {run} is active but could not detach: {error}; it keeps running \
             until `{}`",
            stop_command(addressed)
        ))
    })?;
    let status = control::status_view(addressed);
    let mut lines = vec![format!(
        "managed task `{}` (profile `{}`) is running detached: run {run}",
        addressed.task, addressed.profile
    )];
    for process in &status.processes {
        lines.push(format!("  {:<12} {}", process.name, process.state));
    }
    lines.push(format!(
        "  status: effigy {} status{}",
        addressed.task,
        profile_suffix(addressed)
    ));
    lines.push(format!(
        "  logs:   effigy {} logs <process> --follow{}",
        addressed.task,
        if addressed.profile == effigy_managed::DEFAULT_MANAGED_PROFILE {
            String::new()
        } else {
            format!(" --profile {}", addressed.profile)
        }
    ));
    lines.push(format!("  stop:   {}", stop_command(addressed)));
    Ok(Some(lines.join("\n")))
}

fn next_event(client: &mut SupervisorClient) -> Result<Option<Event>, String> {
    let op = task_engine::operation_context(
        EffectClass::Observe,
        Deadline::At(Instant::now() + TURN),
    );
    match client.event(&op) {
        Ok(event) => Ok(Some(event)),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Err(error.to_string()),
        // The turn's own deadline: nothing arrived.
        Err(_) => Ok(None),
    }
}

fn wait_closed(client: &mut SupervisorClient) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match next_event(client) {
            Ok(Some(Event::Closed { .. })) | Err(_) => return,
            _ => {}
        }
    }
}

fn relay(output: &mut OutputFollower) {
    let (out, err) = output.read();
    if !out.is_empty() {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(&out);
        let _ = stdout.flush();
    }
    if !err.is_empty() {
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(&err);
        let _ = stderr.flush();
    }
}

fn flush() {
    let _ = std::io::stdout().flush();
}

/// The supervisor's pipe closed without its outcome: it died. Its sentinel
/// settles the run; the client reports what the journal says.
fn supervisor_lost(run: RunId, addressed: &Addressed<'_>, detail: &str) -> Closed {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let task_engine::Lookup::Found(record) = task_engine::lookup_run(run) {
            if record.summary.phase == effigy_kernel::run::Phase::Closed {
                return Closed {
                    error: Some(format!(
                        "the supervisor of run {run} exited before reporting ({detail}); its \
                         sentinel settled the run: {}",
                        record.view.state
                    )),
                };
            }
        }
        if Instant::now() >= deadline {
            return Closed {
                error: Some(format!(
                    "the supervisor of run {run} exited before reporting ({detail}); the run \
                     is not closed yet. Settle it with `{}`",
                    stop_command(addressed)
                )),
            };
        }
        std::thread::sleep(task_engine::RUN_POLL);
    }
}

fn finish(addressed: &Addressed<'_>, closed: Closed) -> Result<Option<String>, RunnerError> {
    if let Some(error) = closed.error {
        return Err(RunnerError::task_invocation(error));
    }
    Ok(Some(control::render(&control::status_view(addressed))))
}

fn profile_suffix(addressed: &Addressed<'_>) -> String {
    if addressed.profile == effigy_managed::DEFAULT_MANAGED_PROFILE {
        String::new()
    } else {
        format!(" {}", addressed.profile)
    }
}

pub(in crate::runner) fn stop_command(addressed: &Addressed<'_>) -> String {
    format!("effigy {} stop{}", addressed.task, profile_suffix(addressed))
}
