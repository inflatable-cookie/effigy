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
use std::path::PathBuf;
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

/// Spawns the run's detached supervisor and waits for its admission. A
/// refusal (the run is already live, its predecessor would not settle, a
/// setup error) is reported here; nothing was started.
pub(in crate::runner) fn start(request: &ClientStart<'_>) -> Result<Started, RunnerError> {
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
            admission: None,
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
    offsets: [u64; 2],
    failed: [bool; 2],
    warned: [bool; 2],
}

impl OutputFollower {
    pub(in crate::runner) fn new(root: effigy_kernel::root::StateRoot, run: RunId) -> Self {
        Self {
            root,
            run,
            offsets: [0, 0],
            failed: [false; 2],
            warned: [false; 2],
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
                self.offsets[slot],
                READ_MAX,
            ) {
                let mut bytes = Vec::new();
                if read.from != self.offsets[slot] {
                    bytes.extend_from_slice(
                        b"\n[effigy] output gap: older canonical bytes expired\n",
                    );
                }
                self.offsets[slot] = read.from + read.bytes.len() as u64;
                if read.truncated && !self.warned[slot] {
                    self.warned[slot] = true;
                    bytes.extend_from_slice(
                        b"\n[effigy] capture truncated: inspect the run before restarting\n",
                    );
                }
                if !self.failed[slot] {
                    if std::str::from_utf8(&read.bytes)
                        .is_ok_and(|text| !text.chars().any(|ch| ch.is_control() && ch != '\n'))
                    {
                        bytes.extend_from_slice(&read.bytes);
                    } else {
                        self.failed[slot] = true;
                        bytes.extend_from_slice(b"\n[effigy] output follow stopped: invalid plain text; inspect the run before reopening\n");
                    }
                }
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
}

struct FollowedWorkload {
    name: String,
    sequence: i64,
    lines: [FollowedLines; 2],
}

#[derive(Default)]
pub(super) struct FollowedLines {
    pub(super) offset: u64,
    partial: String,
    failed: bool,
    warned: bool,
}
impl FollowedLines {
    pub(super) fn render(
        &mut self,
        name: &str,
        read: Option<&effigy_kernel::run::LogRead>,
        finish: bool,
    ) -> String {
        let mut rendered = String::new();
        if let Some(read) = read {
            if read.from != self.offset {
                self.partial.clear();
                rendered.push_str(&format!(
                    "\n[{name}] [effigy] output gap: older canonical bytes expired\n"
                ));
            }
            self.offset = read.from + read.bytes.len() as u64;
            if read.truncated && !self.warned {
                self.warned = true;
                rendered.push_str(&format!(
                    "\n[{name}] [effigy] capture truncated: inspect the run before restarting\n"
                ));
            }
            if !self.failed {
                let text = std::str::from_utf8(&read.bytes)
                    .ok()
                    .filter(|text| !text.chars().any(|ch| ch.is_control() && ch != '\n'));
                if let Some(text) = text {
                    for piece in text.split_inclusive('\n') {
                        if self.partial.len() + piece.len() > READ_MAX {
                            self.failed = true;
                            break;
                        }
                        self.partial.push_str(piece);
                        if self.partial.ends_with('\n') {
                            rendered.push_str(&format!("[{name}] {}", self.partial));
                            self.partial.clear();
                        }
                    }
                } else {
                    self.failed = true;
                }
                if self.failed {
                    self.partial.clear();
                    rendered.push_str(&format!("\n[{name}] [effigy] output follow stopped: invalid plain text or line exceeds 64 KiB; inspect the canonical log\n"));
                }
            }
        }
        if finish && !self.partial.is_empty() {
            rendered.push_str(&format!("[{name}] {}\n", self.partial));
            self.partial.clear();
        }
        rendered
    }
}

impl TextFollower {
    fn new(root: effigy_kernel::root::StateRoot, run: RunId, hosts: Vec<HostProcessTab>) -> Self {
        Self {
            output: OutputFollower::new(root.clone(), run),
            root,
            run,
            workloads: Vec::new(),
            hosts: hosts.into_iter().map(HostFollower::new).collect(),
        }
    }

    fn relay(&mut self, finish: bool) {
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
                rendered[slot].push_str(&workload.lines[slot].render(
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
    let mut output = TextFollower::new(root, run, hosts);
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
    fn expired_text_never_completes_a_previous_partial_value() {
        let mut lines = FollowedLines::default();
        assert!(lines
            .render("api", Some(&read(0, "super")), false)
            .is_empty());
        let text = lines.render("api", Some(&read(100, "secret\n")), false);
        assert!(text.contains("output gap") && text.ends_with("[api] secret\n"));
        assert!(!text.contains("supersecret") && lines.partial.is_empty());
        assert!(lines.render("api", Some(&read(107, "")), false).is_empty());
    }
    #[test]
    fn independent_sources_keep_split_writes_in_one_line_and_flush_at_close() {
        let mut api = FollowedLines::default();
        let mut jobs = FollowedLines::default();
        assert!(api.render("api", Some(&read(0, "hel")), false).is_empty());
        assert_eq!(
            jobs.render("jobs", Some(&read(0, "work\n")), false),
            "[jobs] work\n"
        );
        assert_eq!(
            api.render("api", Some(&read(3, "lo\nlast")), false),
            "[api] hello\n"
        );
        assert_eq!(api.render("api", None, true), "[api] last\n");
    }
    #[test]
    fn headless_partial_lines_are_bounded_and_invalid_text_fails_closed_once() {
        for invalid in ["x".repeat(READ_MAX + 1), "super\x1b[31msecret".to_owned()] {
            let mut lines = FollowedLines::default();
            let text = lines.render("api", Some(&read(0, &invalid)), false);
            assert!(text.contains("output follow stopped"));
            assert!(lines.failed && lines.partial.is_empty());
            assert!(!text.contains("supersecret") && !text.contains('\x1b'));
            assert!(lines
                .render("api", Some(&read(invalid.len() as u64, "late\n")), true)
                .is_empty());
        }
    }
    #[test]
    fn partial_plain_text_keeps_spaces_blank_lines_and_utf8() {
        let mut lines = FollowedLines::default();
        assert!(lines
            .render("api", Some(&read(0, "  café")), false)
            .is_empty());
        assert_eq!(
            lines.render("api", Some(&read(7, "\n\nlast")), false),
            "[api]   café\n[api] \n"
        );
        assert_eq!(lines.render("api", None, true), "[api] last\n");
    }
}
