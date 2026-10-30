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
    SessionProcesses, SessionTab,
};

use super::client::{Closed, Started, StopSender};
use super::control::Addressed;
use super::engine::SETTLE_BUDGET;
use super::host_tabs::{HostFollower, HostProcessTab};
use super::wire::{self, ClientFrame, EngineEvent};
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, read_run_log};

const READ_MAX: usize = 64 * 1024;

/// One tab's source: the run's own output, an environment host process's
/// status and log, a workload's log, or a local interactive shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum TabSource {
    Output,
    HostProcess(HostProcessTab),
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
    offsets: [u64; 2],
    lines: [Vec<u8>; 2],
}

struct Shared {
    client: SupervisorClient,
    /// The journal this run writes, bound once: log polls read it directly
    /// (effigy#339).
    root: effigy_kernel::root::StateRoot,
    run: RunId,
    followers: BTreeMap<String, Follower>,
    /// The environment's host processes: their status and live log come from
    /// the files their own supervisors write, not from the run's journal.
    hosts: BTreeMap<String, HostFollower>,
    queue: VecDeque<ProcessEvent>,
    exits: BTreeMap<String, String>,
    startup_finished: bool,
    closed: Option<Closed>,
    stop: StopSender,
}

/// The run's workloads as a TUI session source.
struct RunSource {
    shared: Arc<Mutex<Shared>>,
    shells: Option<ProcessSupervisor>,
    shell_names: Vec<String>,
    /// Set once the TUI's quit path (the run's stop) has begun.
    ending: Arc<AtomicBool>,
}

impl RunSource {
    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_shell(&self, process: &str) -> bool {
        self.shell_names.iter().any(|name| name == process)
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
                                offsets: [0, 0],
                                lines: [Vec::new(), Vec::new()],
                            },
                        );
                        self.exits.remove(&process);
                        self.queue
                            .push_back(event(&process, ProcessEventKind::Started, ""));
                    }
                    Some(EngineEvent::Exited { process, code }) => {
                        // Bytes written before the exit reach the tab first.
                        self.read_logs();
                        let payload = format!("exit={code}");
                        self.exits.insert(process.clone(), payload.clone());
                        self.queue
                            .push_back(event(&process, ProcessEventKind::Exit, &payload));
                    }
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
            if !host.announce_started() {
                self.queue
                    .push_back(event(&name, ProcessEventKind::Started, ""));
            }
            let poll = host.poll();
            for status in poll.statuses {
                self.queue.push_back(ProcessEvent {
                    process: name.clone(),
                    kind: ProcessEventKind::Stdout,
                    payload: format!("[effigy] {}", status.line()),
                    chunk: None,
                });
            }
            for line in poll.lines {
                self.queue.push_back(ProcessEvent {
                    process: name.clone(),
                    kind: ProcessEventKind::Stdout,
                    payload: String::from_utf8_lossy(&line).into_owned(),
                    chunk: None,
                });
            }
        }
    }

    fn read_logs(&mut self) {
        for (name, follower) in &mut self.followers {
            for (slot, (stream, chunk_kind, line_kind)) in [
                (
                    Stream::Stdout,
                    ProcessEventKind::StdoutChunk,
                    ProcessEventKind::Stdout,
                ),
                (
                    Stream::Stderr,
                    ProcessEventKind::StderrChunk,
                    ProcessEventKind::Stderr,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let Observed::Verified(read, _) = read_run_log(
                    &self.root,
                    self.run,
                    follower.sequence,
                    stream,
                    follower.offsets[slot],
                    READ_MAX,
                ) else {
                    continue;
                };
                follower.offsets[slot] = read.from + read.bytes.len() as u64;
                if read.bytes.is_empty() {
                    continue;
                }
                self.queue.push_back(ProcessEvent {
                    process: name.clone(),
                    kind: chunk_kind,
                    payload: String::from_utf8_lossy(&read.bytes).into_owned(),
                    chunk: Some(read.bytes.clone()),
                });
                let buffer = &mut follower.lines[slot];
                buffer.extend_from_slice(&read.bytes);
                while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
                    let line = buffer.drain(..=end).collect::<Vec<u8>>();
                    let text = String::from_utf8_lossy(&line)
                        .trim_end_matches(['\n', '\r'])
                        .to_owned();
                    self.queue.push_back(event(name, line_kind, &text));
                }
            }
        }
    }
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

    fn exit_diagnostics(&self) -> Vec<(String, String)> {
        let mut exits = self
            .shared()
            .exits
            .iter()
            .map(|(name, payload)| (name.clone(), payload.clone()))
            .collect::<Vec<_>>();
        if let Some(shells) = &self.shells {
            exits.extend(shells.exit_diagnostics());
        }
        exits
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
                        offsets: [0, 0],
                        lines: [Vec::new(), Vec::new()],
                    },
                );
                queue.push_back(event(&tab.name, ProcessEventKind::Started, ""));
            }
            TabSource::HostProcess(host) => {
                hosts.insert(tab.name.clone(), HostFollower::new(host.clone()));
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
        ending: ending.clone(),
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
        repo_root,
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
