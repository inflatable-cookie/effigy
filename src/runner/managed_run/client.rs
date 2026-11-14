//! The client of a managed run: it starts the run's detached supervisor
//! and is one of its front ends (§5.8 steps 1, 2 and 6).
//!
//! The text front end follows the run's own output and every workload's
//! (`[name] line`) and waits for the run to close; the detached front end returns once the run is active; the TUI
//! renders every workload. Each is the run's owning client until it
//! detaches: its death, by terminal close, SIGHUP or an agent's SIGKILL,
//! stops and settles the run. INT, TERM and HUP to a client send the stop
//! itself and wait for the settle; a second interrupt within two seconds
//! escalates to KILL.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{Deadline, OpenReason};
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::run::RunError;
use effigy_kernel::supervise::{
    start_engine_at, ClientMode, EngineLaunch, Event, SupervisorClient, OUTPUT_SEQUENCE,
};

use super::control::{self, Addressed};
use super::host_tabs::{HostFollower, HostProcessTab};
use super::output::{Cursor, Output, READ_MAX};
use super::wire::{self, EngineEvent, Front, Payload};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, read_run_log};
use effigy_core::managed_waits::EntryState;

/// How long a client waits for one supervisor event before its next turn.
const TURN: Duration = Duration::from_millis(50);

/// What the client knows when it starts a run's supervisor.
pub(in crate::runner) struct ClientStart<'a> {
    pub(in crate::runner) addressed: Addressed<'a>,
    pub(in crate::runner) invocation: &'a effigy_cli::TaskInvocation,
    pub(in crate::runner) cwd: PathBuf,
    pub(in crate::runner) front: Front,
    pub(in crate::runner) terminal: Option<(u16, u16)>,
    /// Values the client resolved for this checkout's scope (an inherited
    /// alias, say), handed to the supervisor so its workloads inherit the
    /// resolved value and the supervisor repeats no proof it cannot make
    /// (effigy#431 review).
    pub(in crate::runner) resolved_env: &'a [(String, String)],
}

/// A started run: its owning client, until it detaches or the run closes.
pub(in crate::runner) struct Started {
    pub(in crate::runner) client: SupervisorClient,
    pub(in crate::runner) run: RunId,
    /// The journal this run writes, bound once: a follower reads this root and
    /// never re-resolves the run by cwd (effigy#339).
    pub(in crate::runner) root: effigy_kernel::root::StateRoot,
}

#[cfg(all(test, feature = "workspace-serving-fixture"))]
thread_local! {
    static RESERVED_HANDOFF_PRESENT: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(all(test, feature = "workspace-serving-fixture"))]
pub(in crate::runner) fn take_reserved_handoff_observation() -> Option<bool> {
    RESERVED_HANDOFF_PRESENT.with(|observed| observed.take())
}

/// Spawns the run's detached supervisor and waits for its admission. A
/// refusal (the run is already live, its predecessor would not settle, a
/// setup error) is reported here; nothing was started.
pub(in crate::runner) fn start(request: &ClientStart<'_>) -> Result<Started, RunnerError> {
    start_with(request, None)
}

/// [`start`] for the run a workspace dispatcher already reserved (052 §8.11).
///
/// The ticket travels unchanged into the supervisor's private config frame and
/// reaches the engine as `EngineSession::admission()`; the engine then admits
/// exactly the run the reservation names with `Run::begin_admitted`. Nothing
/// here mints an id or admits a second run, and a refusal leaves the
/// reservation for the client's retry to read from the host's own record.
#[allow(dead_code)] // The production channel `Admission` adapter calls this once the
                    // activation supplies the serving owner's negotiation and fence facts.
pub(in crate::runner) fn start_admitted(
    request: &ClientStart<'_>,
    ticket: effigy_kernel::run::AdmissionTicket,
) -> Result<Started, RunnerError> {
    start_with(request, Some(ticket))
}

/// [`start`] for a task the workspace channel re-resolved and reserved.
///
/// The reservation travels unchanged into the supervisor's private config
/// frame, so the engine admits exactly `ticket.run()` with `Run::begin_admitted`
/// under the canonical Task gate. Nothing here mints an id or admits a second
/// run, and the caller keeps the returned owning client for the run's life: a
/// serving owner that drops it stops the run as an owner loss, never as
/// detached success.
#[allow(dead_code)] // The workspace dispatcher's `Admission` is its only caller.
pub(in crate::runner) fn start_reserved(
    host_checkout: &Path,
    invocation: &effigy_cli::TaskInvocation,
    cwd: PathBuf,
    admission: effigy_kernel::run::AdmissionTicket,
    resolution: wire::WorkspaceResolution,
) -> Result<Started, RunnerError> {
    let executable = task_engine::startup_executable()?;
    start_reserved_with_executable(
        host_checkout,
        invocation,
        cwd,
        admission,
        Some(resolution),
        &executable,
    )
}

pub(in crate::runner) fn start_reserved_with_executable(
    host_checkout: &Path,
    invocation: &effigy_cli::TaskInvocation,
    cwd: PathBuf,
    admission: effigy_kernel::run::AdmissionTicket,
    resolution: Option<wire::WorkspaceResolution>,
    executable: &effigy_kernel::supervise::Executable,
) -> Result<Started, RunnerError> {
    let home = task_engine::state_home_path().ok_or_else(|| {
        RunnerError::task_invocation(
            "HOME is unset or relative; Effigy keeps run state under $HOME/.effigy/state",
        )
    })?;
    let payload = Payload {
        task: invocation.name.clone(),
        args: invocation.args.clone(),
        front: Front::Detached,
        state_home: home,
        terminal: None,
        workspace_resolution: resolution,
    };
    let mut env: Vec<(OsString, OsString)> =
        crate::runner::secret_session::workload_ambient_environment()
            .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
            .collect();
    let grant = crate::runner::secret_session::take_child_grant();
    #[cfg(all(test, feature = "workspace-serving-fixture"))]
    RESERVED_HANDOFF_PRESENT.with(|observed| observed.set(Some(grant.is_some())));
    if let Some(grant) = &grant {
        let (key, value) = grant.env_entry();
        env.push((OsString::from(key), OsString::from(value)));
    }
    let cx = task_engine::effect_context(OpenReason::DevServer);
    let root = task_engine::checkout_effects_root(host_checkout)?;
    let run = admission.run();
    let client = start_engine_at(
        &cx,
        root,
        EngineLaunch {
            payload: wire::encode(&payload),
            cwd,
            env,
            handoff: grant.map(|grant| std::sync::Arc::new(grant.into_reader())),
            admission: Some(admission),
        },
        executable,
        ClientMode::Owning,
    )
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "the reserved workspace task `{}` did not start its engine: {error}",
            invocation.name
        ))
    })?;
    Ok(Started {
        client,
        run,
        root: root.clone(),
    })
}

fn start_with(
    request: &ClientStart<'_>,
    admission: Option<effigy_kernel::run::AdmissionTicket>,
) -> Result<Started, RunnerError> {
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
        workspace_resolution: None,
    };
    // The supervisor is Effigy's own process, and it is never a workload's
    // environment: it starts without session access, then reads the one-shot
    // grant its spawn hands it on a descriptor (effigy#338, K12).
    let mut env: Vec<(OsString, OsString)> =
        crate::runner::secret_session::workload_ambient_environment()
            .filter(|(key, _)| key != "EFFIGY_CUSTODY_NONCE" && key != "EFFIGY_PROBE_NONCE")
            .collect();
    let grant = crate::runner::secret_session::take_child_grant();
    if let Some(grant) = &grant {
        let (key, value) = grant.env_entry();
        env.push((OsString::from(key), OsString::from(value)));
    }
    env.extend(
        request
            .resolved_env
            .iter()
            .map(|(key, value)| (OsString::from(key), OsString::from(value))),
    );
    let executable = task_engine::startup_executable()?;
    let cx = task_engine::effect_context(OpenReason::DevServer);
    let root = task_engine::checkout_effects_root(&request.addressed.target.root)?;
    let client = start_engine_at(
        &cx,
        root,
        EngineLaunch {
            payload: wire::encode(&payload),
            cwd: request.cwd.clone(),
            env,
            handoff: grant.map(|grant| std::sync::Arc::new(grant.into_reader())),
            admission,
        },
        &executable,
        ClientMode::Owning,
    )
    .map_err(|error| refusal(&request.addressed, error))?;
    let run = client.run;
    Ok(Started {
        client,
        run,
        root: root.clone(),
    })
}

fn refusal(addressed: &Addressed<'_>, error: RunError) -> RunnerError {
    RunnerError::task_invocation(format!(
        "managed task `{}` (profile `{}`) did not start its supervisor: {error}",
        addressed.task, addressed.profile
    ))
}

/// The run's own output, followed by byte offset (complete, unlike the
/// best-effort event pipe).
pub(in crate::runner) struct OutputFollower {
    root: effigy_kernel::root::StateRoot,
    run: RunId,
    cursors: [Cursor; 2],
}

impl OutputFollower {
    pub(in crate::runner) fn new(root: effigy_kernel::root::StateRoot, run: RunId) -> Self {
        Self {
            root,
            run,
            cursors: Default::default(),
        }
    }

    /// New output since the last read, stdout then stderr.
    pub(in crate::runner) fn read(&mut self) -> (Vec<u8>, Vec<u8>) {
        let mut out = (Vec::new(), Vec::new());
        for (slot, stream) in [Stream::Stdout, Stream::Stderr].into_iter().enumerate() {
            if let Observed::Verified(read, _) = read_run_log(
                &self.root,
                self.run,
                OUTPUT_SEQUENCE,
                stream,
                self.cursors[slot].offset,
                READ_MAX,
            ) {
                let output = self.cursors[slot].consume(&read, false);
                let bytes = render_output(&output).into_bytes();
                if slot == 0 {
                    out.0 = bytes;
                } else {
                    out.1 = bytes;
                }
            }
        }
        out
    }
}

/// The text front end's view: the run's own output as it is, then every
/// workload's output as `[name] line`, as v0.14's headless owner printed it.
/// Lines are emitted whole; a final relay flushes what is left.
struct TextFollower {
    output: OutputFollower,
    root: effigy_kernel::root::StateRoot,
    run: RunId,
    workloads: Vec<FollowedWorkload>,
    /// The environment's host processes: `[name] [host] status` and
    /// `[name] line`, from the logs their own supervisors write.
    hosts: Vec<HostFollower>,
    /// The last wait line printed for each entry, so a live wait is printed
    /// when it advances and a settled one prints once (effigy#458).
    waits: std::collections::BTreeMap<String, String>,
    wait_ledger: crate::runner::managed_wait::WaitLedger,
}

struct FollowedWorkload {
    name: String,
    sequence: i64,
    lines: [Cursor; 2],
}

pub(super) fn render_followed(
    cursor: &mut Cursor,
    name: &str,
    read: Option<&effigy_kernel::run::LogRead>,
    finish: bool,
) -> String {
    let mut output = read
        .map(|read| cursor.consume(read, false))
        .unwrap_or_default();
    let lines = cursor.lines(&mut output, finish);
    render_lines(name, &output, lines)
}

pub(super) fn render_lines(name: &str, output: &Output, lines: Vec<String>) -> String {
    let mut rendered = String::new();
    if output.gap {
        rendered.push_str(&format!(
            "\n[{name}] [effigy] output gap: older canonical bytes expired\n"
        ));
    }
    if output.truncated {
        rendered.push_str(&format!(
            "\n[{name}] [effigy] capture truncated: inspect the run before restarting\n"
        ));
    }
    for line in lines {
        rendered.push_str(&format!("[{name}] {line}\n"));
    }
    if output.failed {
        rendered.push_str(&format!("\n[{name}] [effigy] output follow stopped: invalid plain text or line exceeds 64 KiB; inspect the canonical log\n"));
    }
    rendered
}

pub(super) fn render_output(output: &Output) -> String {
    let mut text = String::new();
    if output.gap {
        text.push_str("\n[effigy] output gap: older canonical bytes expired\n");
    }
    if output.truncated {
        text.push_str("\n[effigy] capture truncated: inspect the run before restarting\n");
    }
    if output.failed {
        text.push_str("\n[effigy] output follow stopped: invalid plain text; inspect the run before reopening\n");
    }
    text.push_str(&output.text);
    text
}

impl TextFollower {
    fn new(
        root: effigy_kernel::root::StateRoot,
        run: RunId,
        hosts: Vec<HostProcessTab>,
        repo: &std::path::Path,
    ) -> Self {
        Self {
            output: OutputFollower::new(root.clone(), run),
            root,
            run,
            workloads: Vec::new(),
            hosts: hosts
                .into_iter()
                .map(|mut host| {
                    host.wait_ledger = host.wait_ledger.for_parent(run);
                    HostFollower::new(host)
                })
                .collect(),
            waits: std::collections::BTreeMap::new(),
            wait_ledger: crate::runner::managed_wait::WaitLedger::run(repo, run),
        }
    }

    /// Prints the entries whose own wait changed: the same line the TUI tab
    /// shows, so the text and interactive planes never disagree.
    fn relay_waits(&mut self, states: impl IntoIterator<Item = EntryState>) {
        let mut lines = String::new();
        for state in states {
            let Some(line) = state.headless_line() else {
                self.waits.remove(&state.entry);
                continue;
            };
            if self.waits.get(&state.entry) == Some(&line) {
                continue;
            }
            self.waits.insert(state.entry.clone(), line.clone());
            lines.push_str(&line);
            lines.push('\n');
        }
        if !lines.is_empty() {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(lines.as_bytes());
            let _ = stdout.flush();
        }
    }

    fn relay(&mut self, finish: bool) {
        if let Some(waits) = self.wait_ledger.read_all() {
            self.relay_waits(waits);
        }
        relay(&mut self.output);
        let mut host_lines = String::new();
        for host in &mut self.hosts {
            let name = host.tab.name.clone();
            let poll = host.poll();
            for status in poll.statuses {
                host_lines.push_str(&format!("[{name}] [host] {}\n", status.line()));
            }
            for line in poll.lines {
                host_lines.push_str(&format!("[{name}] {line}\n"));
            }
        }
        if !host_lines.is_empty() {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(host_lines.as_bytes());
            let _ = stdout.flush();
        }
        // A restart is a new effect: its output follows under the same name.
        if let Observed::Verified(effects, _) = task_engine::run_effects(&self.root, self.run) {
            for view in control::processes(&effects, false) {
                if !self
                    .workloads
                    .iter()
                    .any(|workload| workload.sequence == view.sequence)
                {
                    self.workloads.push(FollowedWorkload {
                        name: view.name,
                        sequence: view.sequence,
                        lines: Default::default(),
                    });
                }
            }
        }
        let mut rendered = [String::new(), String::new()];
        for workload in &mut self.workloads {
            for (slot, stream) in [Stream::Stdout, Stream::Stderr].into_iter().enumerate() {
                let read = read_run_log(
                    &self.root,
                    self.run,
                    workload.sequence,
                    stream,
                    workload.lines[slot].offset,
                    READ_MAX,
                );
                rendered[slot].push_str(&render_followed(
                    &mut workload.lines[slot],
                    &workload.name,
                    read.verified(),
                    finish,
                ));
            }
        }
        if !rendered[0].is_empty() {
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(rendered[0].as_bytes());
            let _ = stdout.flush();
        }
        if !rendered[1].is_empty() {
            let mut stderr = std::io::stderr().lock();
            let _ = stderr.write_all(rendered[1].as_bytes());
            let _ = stderr.flush();
        }
    }
}

/// How the client's view of the run ended.
pub(in crate::runner) struct Closed {
    pub(in crate::runner) error: Option<String>,
    /// The exit status a step's own code decided, when `error` is set.
    pub(in crate::runner) code: Option<i32>,
}

impl Closed {
    /// The client's result: a failed run's error, exiting with the step's
    /// own code when one decided it (as v0.14 did).
    pub(in crate::runner) fn into_error(self) -> Option<RunnerError> {
        let error = self.error?;
        Some(match self.code {
            Some(code) if code != 1 => RunnerError::RunControl {
                code,
                details_json: serde_json::json!({ "error": error, "exit_code": code }).to_string(),
                detail: error,
            },
            _ => RunnerError::task_invocation(error),
        })
    }
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
    hosts: Vec<HostProcessTab>,
) -> Result<Option<String>, RunnerError> {
    task_engine::install_client_router()?;
    let Started {
        mut client,
        run,
        root,
    } = started;
    let _ = writeln!(
        std::io::stdout(),
        "[managed] `{}` (profile `{}`): run {run}; stop with `{}`, or interrupt this client",
        addressed.task,
        addressed.profile,
        stop_command(addressed)
    );
    flush();
    let mut output = TextFollower::new(root, run, hosts, &addressed.target.root);
    let mut stop = StopSender::new();
    let closed = loop {
        if !task_engine::routed_signals().is_empty() {
            stop.send(&client);
        }
        output.relay(false);
        match next_event(&mut client) {
            Ok(Some(Event::Engine(bytes))) => {
                if let Some(EngineEvent::Outcome { error, code }) = wire::decode(&bytes) {
                    output.relay(false);
                    // Closed follows the outcome on the same pipe.
                    wait_closed(&mut client);
                    output.relay(true);
                    break Closed { error, code };
                }
            }
            Ok(Some(Event::Closed { .. })) => {
                output.relay(true);
                break Closed {
                    error: None,
                    code: None,
                };
            }
            Ok(_) => {}
            Err(lost) => {
                output.relay(true);
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
    let Started {
        mut client,
        run,
        root,
    } = started;
    let mut output = OutputFollower::new(root, run);
    let mut stop = StopSender::new();
    loop {
        if !task_engine::routed_signals().is_empty() {
            // Interrupting a detaching client stops the run it started.
            stop.send(&client);
        }
        match next_event(&mut client) {
            Ok(Some(Event::Active)) if !stop.requested() => break,
            Ok(Some(Event::Engine(bytes))) => {
                if let Some(EngineEvent::Outcome { error, code }) = wire::decode(&bytes) {
                    wait_closed(&mut client);
                    relay(&mut output);
                    return finish(addressed, Closed { error, code });
                }
            }
            Ok(Some(Event::Closed { .. })) => {
                relay(&mut output);
                return finish(
                    addressed,
                    Closed {
                        error: None,
                        code: None,
                    },
                );
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
    let op = task_engine::reading_context(Deadline::At(Instant::now() + TURN));
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
        if let task_engine::Lookup::Found(record) =
            task_engine::lookup_checkout(&addressed.target.root, run)
        {
            if record.closed() {
                return Closed {
                    error: Some(format!(
                        "the supervisor of run {run} exited before reporting ({detail}); its \
                         sentinel settled the run: {}",
                        record.view.state
                    )),
                    code: None,
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
                code: None,
            };
        }
        std::thread::sleep(task_engine::RUN_POLL);
    }
}

fn finish(addressed: &Addressed<'_>, closed: Closed) -> Result<Option<String>, RunnerError> {
    if let Some(error) = closed.into_error() {
        return Err(error);
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
    format!(
        "effigy {} stop{}",
        addressed.task,
        profile_suffix(addressed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn read(from: u64, text: &str) -> effigy_kernel::run::LogRead {
        effigy_kernel::run::LogRead {
            from,
            bytes: text.as_bytes().to_vec(),
            end: from + text.len() as u64,
            truncated: false,
        }
    }
    #[test]
    fn text_adapters_preserve_prefixes_and_diagnostics() {
        let mut lines = Cursor::default();
        assert!(render_followed(&mut lines, "api", Some(&read(0, "super")), false).is_empty());
        let text = render_followed(&mut lines, "api", Some(&read(100, "secret\n\nlast")), false);
        assert_eq!(
            text,
            "\n[api] [effigy] output gap: older canonical bytes expired\n[api] secret\n[api] \n"
        );
        assert_eq!(
            render_followed(&mut lines, "api", None, true),
            "[api] last\n"
        );
        let text = render_followed(&mut lines, "api", Some(&read(112, "bad\x1btext")), false);
        assert!(text.contains("output follow stopped") && !text.contains('\x1b'));
        assert!(render_followed(&mut lines, "api", Some(&read(120, "late\n")), true).is_empty());
    }
}
