//! The TUI front end of a managed run (I-SV3): it renders the run's logs,
//! read by offset from the journal's log files, and sends its owning
//! client's input, restart and stop commands. Shell tabs are the human's
//! interactive terminals, so they run here, beside the TUI, and never in the
//! run. Quitting, closing the terminal or killing the client stops and
//! settles the run.

use std::collections::{BTreeMap, VecDeque};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use effigy_core::managed_waits::EntryState;
use effigy_kernel::ctx::Deadline;
use effigy_kernel::custody::Stream;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::supervise::{Event, SupervisorClient, OUTPUT_SEQUENCE};
use effigy_process::{
    ProcessEvent, ProcessEventKind, ProcessManagerError, ProcessSpec, ProcessSupervisor,
    ShutdownProgress,
};
use effigy_tui::multiprocess::{
    run_multiprocess_tui_with_source, MultiProcessTuiControl, MultiProcessTuiOptions,
    SessionEntryState, SessionProcesses, SessionTab,
};

use super::client::{Closed, Started, StopSender};
use super::control::Addressed;
use super::engine::SETTLE_BUDGET;
use super::host_tabs::{HostFollower, HostProcessTab};
use super::output::{Cursor, READ_MAX};
use super::wire::{self, ClientFrame, EngineEvent};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, read_run_log};

/// One tab's source: the run's own output, an environment host process's
/// status and log, a workload's log, or a local interactive shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum TabSource {
    Output,
    HostProcess(Box<HostProcessTab>),
    Workload,
    Shell,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct ManagedTab {
    pub(in crate::runner) name: String,
    pub(in crate::runner) source: TabSource,
    pub(in crate::runner) pty: bool,
    pub(in crate::runner) shutdown_on_exit: bool,
}

struct Follower {
    sequence: i64,
    cursors: [Cursor; 2],
    // None means the independent output tab or an exited workload.
    ready_through: Option<u64>,
}

struct Shared {
    client: SupervisorClient,
    /// The journal this run writes, bound once: log polls read it directly
    /// (effigy#339).
    root: effigy_kernel::root::StateRoot,
    run: RunId,
    followers: BTreeMap<String, Follower>,
    /// The environment's host processes, with generation-bound observations
    /// and live canonical logs from their own supervisors.
    hosts: BTreeMap<String, HostFollower>,
    queue: VecDeque<ProcessEvent>,
    exits: BTreeMap<String, SessionEntryState>,
    /// Log offset already delivered as live styled output per tab, so the log
    /// follower does not repeat those bytes plain.
    styled_through: BTreeMap<String, u64>,
    startup_finished: bool,
    closed: Option<Closed>,
    stop: StopSender,
}

/// The run's workloads as a TUI session source.
struct RunSource {
    shared: Arc<Mutex<Shared>>,
    shells: Option<ProcessSupervisor>,
    shell_names: Vec<String>,
    startup_tab: Option<String>,
    sequence_focus: Mutex<(Instant, Option<String>)>,
    /// Set once the TUI's quit path (the run's stop) has begun.
    ending: Arc<AtomicBool>,
    /// Where the run's workloads publish their waits, and which tabs they
    /// are, so a front end that attached late still names every wait.
    wait_ledger: crate::runner::managed_wait::WaitLedger,
}

impl RunSource {
    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn sequence_gate(
        &self,
        root: &effigy_kernel::root::StateRoot,
        parent: RunId,
    ) -> Option<String> {
        let mut cached = self
            .sequence_focus
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cached.0.elapsed() >= Duration::from_millis(500) {
            if let Some(observed) = crate::runner::managed_wait::sequence_gate(root, parent) {
                cached.1 = observed;
            }
            cached.0 = Instant::now();
        }
        cached.1.clone()
    }

    fn is_shell(&self, process: &str) -> bool {
        self.shell_names.iter().any(|name| name == process)
    }

    /// Every entry's own wait: the run's published state, the environment's
    /// host-process records, and the run's own record for a tab that attached
    /// after its event.
    fn wait_states(&self) -> Vec<EntryState> {
        let shared = self.shared();
        let mut states = self
            .wait_ledger
            .read_all()
            .unwrap_or_default()
            .into_iter()
            .filter(EntryState::holds_a_wait)
            .collect::<Vec<_>>();
        for (tab_name, host) in &shared.hosts {
            let Some(state) = host.wait_state() else {
                continue;
            };
            let mut state = state.clone();
            // The tab may be renamed beside a workload of the same name.
            state.entry = tab_name.clone();
            states.push(state);
        }
        states
    }

    fn frame(&self, frame: &ClientFrame) -> Result<(), ProcessManagerError> {
        let process = match frame {
            ClientFrame::Input { process, .. }
            | ClientFrame::Restart { process }
            | ClientFrame::Terminate { process } => process.clone(),
        };
        self.shared()
            .client
            .send_frame(&wire::encode(frame))
            .map_err(|error| ProcessManagerError::InputWrite { process, error })
    }
}

impl Shared {
    /// One turn: supervisor events, then new log bytes for every tab.
    fn pump(&mut self) {
        let op =
            task_engine::reading_context(Deadline::At(Instant::now() + Duration::from_millis(5)));
        loop {
            match self.client.event(&op) {
                Ok(Event::Engine(bytes)) => match wire::decode::<EngineEvent>(&bytes) {
                    Some(EngineEvent::Started { process, sequence }) => {
                        self.followers.insert(
                            process.clone(),
                            Follower {
                                sequence,
                                cursors: Default::default(),
                                ready_through: Some(0),
                            },
                        );
                        self.exits.remove(&process);
                        // A restart writes a fresh log from offset zero, so no
                        // earlier style coverage applies to it.
                        self.styled_through.remove(&process);
                        self.queue
                            .push_back(event(&process, ProcessEventKind::Started, ""));
                    }
                    Some(EngineEvent::Exited { process, code }) => {
                        // The final read includes bytes committed after the last poll.
                        if let Some(follower) = self.followers.get_mut(&process) {
                            follower.ready_through = None;
                        }
                        self.read_logs();
                        let payload = format!("exit={code}");
                        self.exits
                            .insert(process.clone(), SessionEntryState::from_exit_code(code));
                        self.queue
                            .push_back(event(&process, ProcessEventKind::Exit, &payload));
                    }
                    Some(EngineEvent::Styled { process, chunk }) => {
                        let end = chunk.offset
                            + chunk
                                .spans
                                .iter()
                                .map(|span| span.text.len() as u64)
                                .sum::<u64>();
                        let cursor = self
                            .followers
                            .get(&process)
                            .map(|follower| follower.cursors[0].offset)
                            .unwrap_or(0);
                        let through = self.styled_through.entry(process.clone()).or_insert(0);
                        if !accept_style(through, cursor, chunk.offset, end) {
                            continue;
                        }
                        self.queue.push_back(event(
                            &process,
                            ProcessEventKind::Styled,
                            &serde_json::to_string(&chunk).expect("typed style event"),
                        ));
                    }
                    Some(EngineEvent::OutputReady { process, through }) => {
                        if let Some(follower) = self.followers.get_mut(&process) {
                            if let Some(ready) = &mut follower.ready_through {
                                *ready = (*ready).max(through);
                            }
                        }
                    }
                    Some(EngineEvent::WaitsChanged) => {}
                    Some(EngineEvent::StartupFinished) => self.startup_finished = true,
                    Some(EngineEvent::Outcome { error, code }) => {
                        self.closed = Some(Closed { error, code });
                    }
                    _ => {}
                },
                Ok(Event::Closed { .. }) => {
                    self.startup_finished = true;
                    if self.closed.is_none() {
                        self.closed = Some(Closed {
                            error: None,
                            code: None,
                        });
                    }
                }
                Ok(Event::Active) => self.startup_finished = true,
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.startup_finished = true;
                    if self.closed.is_none() {
                        self.closed = Some(Closed {
                            error: Some(format!(
                                "the supervisor of run {} exited before reporting; its \
                                 sentinel settles the run",
                                self.run
                            )),
                            code: None,
                        });
                    }
                    break;
                }
                Err(_) => break,
            }
        }
        self.read_logs();
        self.poll_host_processes();
    }

    /// One status/log poll per environment host process. A tab starts with its
    /// current status, then receives every transition and new log line.
    fn poll_host_processes(&mut self) {
        let names = self.hosts.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let Some(host) = self.hosts.get_mut(&name) else {
                continue;
            };
            // The status poll also reads the process's published wait, so a
            // blocked host is announced as started only once that wait is
            // over (effigy#458).
            let poll = host.poll();
            if host.may_announce_started() && !host.announce_started() {
                self.queue
                    .push_back(event(&name, ProcessEventKind::Started, ""));
            }
            for status in poll.statuses {
                self.queue.push_back(ProcessEvent {
                    process: name.clone(),
                    kind: ProcessEventKind::Stdout,
                    payload: format!("[effigy] {}", status.line()),
                    chunk: None,
                });
            }
            for chunk in poll.styled {
                self.queue.push_back(event(
                    &name,
                    ProcessEventKind::Styled,
                    &serde_json::to_string(&chunk).expect("typed host style event"),
                ));
            }
        }
    }

    fn read_logs(&mut self) {
        for (name, follower) in &mut self.followers {
            for (slot, stream) in [Stream::Stdout, Stream::Stderr].into_iter().enumerate() {
                let max = readable_bytes(follower.ready_through, follower.cursors[slot].offset);
                if max == 0 {
                    continue;
                }
                let Observed::Verified(read, _) = read_run_log(
                    &self.root,
                    self.run,
                    follower.sequence,
                    stream,
                    follower.cursors[slot].offset,
                    max,
                ) else {
                    continue;
                };
                let mut output = follower.cursors[slot].consume(&read, false);
                if output.truncated {
                    self.queue.push_back(event(name, ProcessEventKind::Stdout, "[effigy] capture truncated: log append failed; inspect this run before restarting"));
                }
                if output.failed {
                    self.queue.push_back(event(name, ProcessEventKind::Stdout, "[effigy] output follow stopped: invalid plain UTF-8 or control bytes; restart the producer"));
                }
                if output.gap {
                    self.queue.push_back(event(name, ProcessEventKind::Stdout, "[effigy] output gap: older canonical bytes expired; resuming the retained log"));
                }
                // Bytes already sent as live styles keep their colour; only
                // the rest of a followed log renders as plain text.
                let covered = self.styled_through.get(name).copied().unwrap_or(0);
                if let Some(cut) = covered.checked_sub(output.from).filter(|cut| *cut > 0) {
                    let cut = cut as usize;
                    if cut >= output.text.len() {
                        continue;
                    }
                    output.text = output.text[cut..].to_owned();
                    output.from = covered;
                }
                if output.text.is_empty() {
                    continue;
                }
                let chunk = output.chunk();
                self.queue.push_back(event(
                    name,
                    ProcessEventKind::Styled,
                    &serde_json::to_string(&chunk).expect("typed style event"),
                ));
            }
        }
    }
}

// A live workload only follows the extent whose styles were offered first.
fn readable_bytes(ready: Option<u64>, cursor: u64) -> usize {
    ready.map_or(READ_MAX, |end| {
        end.saturating_sub(cursor).min(READ_MAX as u64) as usize
    })
}

// Only suppress canonical bytes that were actually delivered, in order.
fn accept_style(through: &mut u64, cursor: u64, start: u64, end: u64) -> bool {
    if start != (*through).max(cursor) || end < start {
        return false;
    }
    *through = end;
    true
}

fn event(process: &str, kind: ProcessEventKind, payload: &str) -> ProcessEvent {
    ProcessEvent {
        process: process.to_owned(),
        kind,
        payload: payload.to_owned(),
        chunk: None,
    }
}

impl SessionProcesses for RunSource {
    fn startup_focus(&self) -> Option<String> {
        let shared = self.shared();
        if shared.closed.is_some() {
            return None;
        }
        let parent = shared.run;
        let root = shared.root.clone();
        let startup_finished = shared.startup_finished;
        drop(shared);
        if let Some(tab) = self.sequence_gate(&root, parent) {
            return Some(tab);
        }
        if startup_finished {
            return None;
        }
        self.wait_ledger.read(super::OUTPUT_LOG).and_then(|state| {
            (state.phase == effigy_core::managed_waits::EntryPhase::Starting)
                .then(|| self.startup_tab.clone())
                .flatten()
        })
    }

    fn entry_waits(&self) -> Vec<EntryState> {
        self.wait_states()
    }

    fn next_event_timeout(&self, timeout: Duration) -> Option<ProcessEvent> {
        if let Some(shells) = &self.shells {
            if let Some(event) = shells.next_event_timeout(Duration::ZERO) {
                return Some(event);
            }
        }
        let deadline = Instant::now() + timeout;
        loop {
            {
                let mut shared = self.shared();
                if let Some(event) = shared.queue.pop_front() {
                    return Some(event);
                }
                shared.pump();
                if let Some(event) = shared.queue.pop_front() {
                    return Some(event);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn send_input(&self, process: &str, input: &str) -> Result<(), ProcessManagerError> {
        if self.is_shell(process) {
            return match &self.shells {
                Some(shells) => shells.send_input(process, input),
                None => Ok(()),
            };
        }
        self.frame(&ClientFrame::Input {
            process: process.to_owned(),
            bytes: input.as_bytes().to_vec(),
        })
    }

    fn terminate_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        if self.is_shell(process) {
            return match &self.shells {
                Some(shells) => shells.terminate_process(process),
                None => Ok(()),
            };
        }
        self.frame(&ClientFrame::Terminate {
            process: process.to_owned(),
        })
    }

    fn restart_process(&self, process: &str) -> Result<(), ProcessManagerError> {
        if self.is_shell(process) {
            return match &self.shells {
                Some(shells) => shells.restart_process(process),
                None => Ok(()),
            };
        }
        self.frame(&ClientFrame::Restart {
            process: process.to_owned(),
        })
    }

    fn startup_finished(&self) -> bool {
        let shells = self
            .shells
            .as_ref()
            .is_none_or(ProcessSupervisor::startup_finished);
        shells && self.shared().startup_finished
    }

    fn cancel_startup(&self) {
        if let Some(shells) = &self.shells {
            shells.cancel_startup();
        }
    }

    fn take_startup_error(&self) -> Option<ProcessManagerError> {
        self.shells
            .as_ref()
            .and_then(ProcessSupervisor::take_startup_error)
    }

    fn entry_states(&self) -> Vec<(String, SessionEntryState)> {
        // The run's own engine reports an entry's exit as the typed code it
        // recorded; the shell processes this client supervises report theirs.
        let mut entries = self
            .shared()
            .exits
            .iter()
            .map(|(name, state)| (name.clone(), state.clone()))
            .collect::<Vec<_>>();
        if let Some(shells) = &self.shells {
            entries.extend(shells.entry_states());
        }
        entries
    }

    /// Quit is the run's stop: one request, then the run's own settle,
    /// observed until it closes. Local shells end with the client.
    fn terminate_all_graceful_with_progress(
        &self,
        timeout: Duration,
        on_progress: &mut dyn FnMut(ShutdownProgress),
    ) {
        self.ending.store(true, Ordering::Release);
        if let Some(shells) = &self.shells {
            shells.terminate_all_graceful(timeout);
        }
        on_progress(ShutdownProgress::SendingTerm);
        {
            let mut shared = self.shared();
            if shared.closed.is_none() {
                let Shared { client, stop, .. } = &mut *shared;
                stop.send(client);
            }
        }
        on_progress(ShutdownProgress::Waiting);
        let deadline = Instant::now() + SETTLE_BUDGET + Duration::from_secs(30);
        while Instant::now() < deadline {
            let mut shared = self.shared();
            if shared.closed.is_some() {
                break;
            }
            shared.pump();
            shared.queue.clear();
            drop(shared);
            std::thread::sleep(Duration::from_millis(20));
        }
        on_progress(ShutdownProgress::Complete {
            total: self.shared().followers.len(),
            forced: 0,
        });
    }
}

/// How long a closed terminal may go unnoticed by the TUI's own loop.
const HANGUP_GRACE: Duration = Duration::from_secs(2);

/// Backstop for a closed terminal. The TUI notices a hangup itself and quits,
/// which stops and settles the run. If the hangup lands while crossterm is
/// inside its own read, crossterm retries the zero-byte read forever and the
/// loop never returns. Then this client exits: its death is the run's stop
/// (the supervisor settles it as owner-lost), as for any dead client.
fn hangup_watchdog(ending: Arc<AtomicBool>) {
    // Only a terminal hangs up; a pipe at EOF also polls POLLHUP.
    // SAFETY: isatty on this process's standard input.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("effigy-tui-hangup".into())
        .spawn(move || {
            while !ending.load(Ordering::Acquire) {
                if stdin_hung_up() {
                    let deadline = Instant::now() + HANGUP_GRACE;
                    while Instant::now() < deadline {
                        if ending.load(Ordering::Acquire) {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    // 128 + SIGHUP, the status of a client its terminal hung up.
                    std::process::exit(129);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
}

fn stdin_hung_up() -> bool {
    let mut input = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one initialized pollfd for this process's stdin; no wait.
    let ready = unsafe { libc::poll(&mut input, 1, 0) };
    ready > 0 && input.revents & (libc::POLLHUP | libc::POLLNVAL) != 0
}

/// Runs the TUI over a started run until it closes.
/// The initial complete wait snapshot is in the admitted run's journal.
pub(in crate::runner) fn attach_tui(
    started: Started,
    addressed: &Addressed<'_>,
    repo_root: PathBuf,
    tabs: Vec<ManagedTab>,
    tab_order: Vec<String>,
    shells: Vec<ProcessSpec>,
) -> Result<Option<String>, RunnerError> {
    task_engine::install_client_router()?;
    let Started { client, run, root } = started;
    let mut followers = BTreeMap::new();
    let mut hosts = BTreeMap::new();
    let mut queue = VecDeque::new();
    for tab in &tabs {
        match &tab.source {
            TabSource::Output => {
                followers.insert(
                    tab.name.clone(),
                    Follower {
                        sequence: OUTPUT_SEQUENCE,
                        cursors: Default::default(),
                        ready_through: None,
                    },
                );
                queue.push_back(event(&tab.name, ProcessEventKind::Started, ""));
            }
            TabSource::HostProcess(host) => {
                let mut host = (**host).clone();
                host.wait_ledger = host.wait_ledger.for_parent(run);
                hosts.insert(tab.name.clone(), HostFollower::new(host));
            }
            TabSource::Workload | TabSource::Shell => {}
        }
    }
    let shared = Arc::new(Mutex::new(Shared {
        client,
        root,
        run,
        followers,
        hosts,
        queue,
        exits: BTreeMap::new(),
        styled_through: BTreeMap::new(),
        startup_finished: false,
        closed: None,
        stop: StopSender::new(),
    }));
    let shell_names = shells.iter().map(|shell| shell.name.clone()).collect();
    let ending = Arc::new(AtomicBool::new(false));
    let source = RunSource {
        shared: shared.clone(),
        shells: (!shells.is_empty())
            .then(|| ProcessSupervisor::spawn_progressively(repo_root.clone(), shells)),
        shell_names,
        sequence_focus: Mutex::new((Instant::now() - Duration::from_secs(1), None)),
        startup_tab: tabs
            .iter()
            .find(|tab| matches!(tab.source, TabSource::Output))
            .map(|tab| tab.name.clone()),
        ending: ending.clone(),
        wait_ledger: crate::runner::managed_wait::WaitLedger::run(&addressed.target.root, run),
    };
    hangup_watchdog(ending.clone());
    let session_tabs = tabs
        .iter()
        .map(|tab| SessionTab {
            name: tab.name.clone(),
            pty: tab.pty,
            shutdown_on_exit: tab.shutdown_on_exit,
        })
        .collect();
    // INT, TERM or HUP to the client ends the TUI, which stops the run; so
    // does its terminal closing (the TUI's own input wait sees the hangup).
    let poll = |_: &dyn SessionProcesses| -> Result<bool, String> {
        Ok(!task_engine::routed_signals().is_empty()
            || shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .closed
                .is_some())
    };
    // Hide existing progress before the TUI draws, then retire it before
    // shutdown takes ownership of the status area.
    let terminal = effigy_ui::steps::suspend_terminal();
    let retire = |_: &dyn SessionProcesses| -> Result<(), String> {
        terminal.retire_running();
        Ok(())
    };
    let none = |_: &dyn SessionProcesses| -> Result<(), String> { Ok(()) };
    let control = MultiProcessTuiControl {
        poll: &poll,
        before_shutdown: &retire,
        after_shutdown: &none,
    };
    let rendered = run_multiprocess_tui_with_source(
        Box::new(source),
        session_tabs,
        tab_order,
        MultiProcessTuiOptions::default(),
        Some(&control),
    );
    drop(terminal);
    ending.store(true, Ordering::Release);
    let closed = shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .closed
        .take();
    if let Err(error) = rendered {
        // The run is already stopping or stopped through the source's
        // shutdown; report the terminal failure after its outcome.
        // The terminal may be gone (client death): never panic writing.
        let _ = writeln!(std::io::stderr(), "warning: managed TUI: {error}");
    }
    match closed.and_then(Closed::into_error) {
        Some(error) => Err(error),
        None => Ok(Some(super::control::render(&super::control::status_view(
            addressed,
        )))),
    }
}

#[cfg(test)]
mod style_coverage_tests {
    use super::{accept_style, readable_bytes};

    #[test]
    fn committed_bytes_wait_for_live_style_before_plain_following() {
        let mut through = 0;
        let mut cursor = 0;
        // The producer commits bytes before the engine publishes its event.
        assert_eq!(readable_bytes(Some(0), cursor), 0);
        assert!(accept_style(&mut through, cursor, 0, 5));
        // The engine publishes the watermark only after the style event.
        assert_eq!(readable_bytes(Some(5), cursor), 5);
        cursor += 5;
        assert_eq!(through, cursor); // the plain follower suppresses these bytes
        assert_eq!(readable_bytes(Some(5), cursor), 0);
        // A missing style event is recovered as plain at the next watermark.
        assert_eq!(readable_bytes(Some(9), cursor), 4);
    }

    #[test]
    fn evicted_prefix_falls_back_to_plain_log_before_styles_resume() {
        let mut through = 0;
        assert!(!accept_style(&mut through, 0, 4, 8));
        assert_eq!(through, 0);
        // The follower delivers the missing canonical bytes through offset 8.
        assert!(accept_style(&mut through, 8, 8, 12));
        assert_eq!(through, 12);
        assert!(!accept_style(&mut through, 8, 4, 8));
    }
}
