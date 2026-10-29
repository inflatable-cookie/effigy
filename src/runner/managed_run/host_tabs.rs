//! The environment's host processes as a managed session sees them
//! (effigy#322).
//!
//! A managed run bound to an environment shows the environment's
//! `[[containers.<env>.host_processes]]` entries beside its workloads: one tab
//! per process with a status line and its live log. The processes are
//! environment-owned kernel runs started by the environment's `up` (contract
//! 052 §10.2), so this module owns only what a front end needs:
//!
//! - where the supervisor writes the process's output
//!   ([`crate::runner::host_process::host_process_log_path`]) and, for a
//!   listener, the state file that says when it is ready;
//! - the status line, derived from the supervisor's own log lines and that
//!   state file, never from scraping a process tree;
//! - [`LogTail`], a bounded incremental reader for the log file, shared by the
//!   TUI tabs and the `--headless` front end so both show the same lines.

use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_containers::{EffectiveContainerPolicy, HostProcessRestart};
use effigy_tui::waits::{EntryPhase, EntryState};
use serde::Deserialize;

/// The most one log tail read returns, so a chatty process cannot stall a
/// frame or a headless turn.
const READ_MAX: usize = 64 * 1024;
/// How often a front end asks a listener's state file whether it is ready.
const LISTENER_POLL: Duration = Duration::from_millis(250);

/// One environment host process, as a tab and as a followed log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessTab {
    pub(in crate::runner) name: String,
    pub(in crate::runner) log: PathBuf,
    /// The state file a listener publishes; `None` for a plain process.
    pub(in crate::runner) listener_state: Option<PathBuf>,
    pub(in crate::runner) restart: HostProcessRestart,
    pub(in crate::runner) restart_delay_ms: u64,
    /// The scope this process's wait is published in. A read resolves the
    /// owner that currently publishes it, so a tab never reads a publisher
    /// that lost the scope (effigy#458).
    pub(in crate::runner) wait_ledger: crate::runner::managed_wait::WaitLedger,
}

impl HostProcessTab {
    /// Every declared host process of `policy`, in declaration order.
    pub(in crate::runner) fn for_environment(
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
    ) -> Vec<Self> {
        policy
            .host_processes
            .iter()
            .map(|process| Self {
                name: process.name.clone(),
                log: crate::runner::host_process::host_process_log_path(
                    repo_root,
                    policy,
                    &process.name,
                ),
                listener_state: process.listener.as_ref().map(|_| {
                    crate::runner::host_process::host_process_listener_state_path(
                        repo_root,
                        policy,
                        &process.name,
                    )
                }),
                restart: process.restart,
                restart_delay_ms: process.restart_delay_ms,
                wait_ledger: crate::runner::managed_wait::WaitLedger::environment(
                    repo_root,
                    &policy.name,
                ),
            })
            .collect()
    }

    /// Whether the declared restart policy replaces this process after it
    /// exits with `code`.
    fn restarts_after(&self, code: i32) -> bool {
        match self.restart {
            HostProcessRestart::Always => true,
            HostProcessRestart::OnFailure => code != 0,
            HostProcessRestart::Never => false,
        }
    }
}

/// What a host process's status line says, in the session's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum HostStatus {
    /// The supervisor has not reported a generation yet.
    Starting,
    /// A generation is running; a listener that is not serving yet waits here.
    Running,
    /// A listener reported its route: its live URL.
    Ready(String),
    /// The last generation exited and the restart policy replaces it.
    Restarting,
    /// The last generation exited and nothing replaces it.
    Exited(i32),
    /// The bring-up has not started this process yet, and names what it waits
    /// for.
    Waiting(String),
    /// A prerequisite failed, so this process never started.
    NotStarted(String),
}

impl HostStatus {
    /// Whether this status is evidence that a process actually started. A
    /// missing wait record is no evidence either way, so only a supervisor's
    /// own report or a settled record may announce a start (effigy#458).
    fn reports_a_start(&self) -> bool {
        matches!(
            self,
            Self::Running | Self::Ready(_) | Self::Restarting | Self::Exited(_)
        )
    }

    /// The line a tab or the headless front end shows for this status.
    pub(in crate::runner) fn line(&self) -> String {
        match self {
            Self::Starting => "starting".to_owned(),
            Self::Running => "running".to_owned(),
            Self::Ready(url) => format!("ready on {url}"),
            Self::Restarting => "restarting".to_owned(),
            Self::Exited(code) => format!("exited {code}"),
            Self::Waiting(what) => what.clone(),
            Self::NotStarted(what) => what.clone(),
        }
    }
}

/// The status a supervisor log line reports, if it reports one. The lines are
/// the supervisor's own (`host_process.rs` writes them into the same log the
/// tab follows), so this reads a stable internal contract, never process
/// output a user's program printed.
pub(in crate::runner) fn status_from_log_line(
    tab: &HostProcessTab,
    line: &str,
) -> Option<HostStatus> {
    const MARKERS: &str = "[effigy host-process]";
    let reported = line.trim();
    if !reported.starts_with(MARKERS) {
        return None;
    }
    if reported.contains("started `") {
        return Some(HostStatus::Running);
    }
    if reported.contains("failed to spawn") {
        return Some(HostStatus::Exited(127));
    }
    let code = reported
        .split("exited (code ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|code| code.trim().parse::<i32>().ok())?;
    Some(if tab.restarts_after(code) {
        HostStatus::Restarting
    } else {
        HostStatus::Exited(code)
    })
}

/// A listener's live URL, once its state file says it is ready. A state file
/// that is absent, mid-write or for a failed generation is no observation.
pub(in crate::runner) fn ready_url(tab: &HostProcessTab) -> Option<String> {
    #[derive(Deserialize)]
    struct ListenerState {
        status: String,
        internal_url: Option<String>,
        public_url: Option<String>,
    }
    let path = tab.listener_state.as_ref()?;
    let raw = std::fs::read(path).ok()?;
    let state: ListenerState = serde_json::from_slice(&raw).ok()?;
    if state.status != "ready" {
        return None;
    }
    state.public_url.or(state.internal_url)
}

/// One host process's followed log and status, as a front end renders it.
#[derive(Debug)]
pub(in crate::runner) struct HostFollower {
    pub(in crate::runner) tab: HostProcessTab,
    tail: LogTail,
    status: HostStatus,
    announced: bool,
    /// Whether the tab has been told this process exists, so its startup
    /// projection stops waiting for it.
    started: bool,
    next_poll: std::time::Instant,
    /// The bring-up's published wait, while it holds this process.
    wait: Option<EntryState>,
    /// Whether the published record says the wait is over: the planner only
    /// settles an entry once it launched the process.
    settled: bool,
}

impl HostFollower {
    pub(in crate::runner) fn new(tab: HostProcessTab) -> Self {
        Self {
            tab,
            tail: LogTail::new(),
            status: HostStatus::Starting,
            announced: false,
            started: false,
            next_poll: std::time::Instant::now(),
            wait: None,
            settled: false,
        }
    }

    /// Marks this host process as announced to the session. `true` when it
    /// already was.
    pub(in crate::runner) fn announce_started(&mut self) -> bool {
        std::mem::replace(&mut self.started, true)
    }

    /// Whether this process may be announced as started. It needs evidence
    /// that it did start — the supervisor's own report, or a published record
    /// the planner settled after launching it. A missing record, including the
    /// window before a publisher stages its snapshot, is no evidence, so a tab
    /// never claims a process ran because its wait is not visible yet
    /// (effigy#458).
    pub(in crate::runner) fn may_announce_started(&self) -> bool {
        self.wait_state().is_none() && (self.settled || self.status.reports_a_start())
    }

    /// The wait the bring-up published for this process, if it still holds
    /// it: the dependency and its phase, the listener's own probe, or the
    /// prerequisite that failed.
    pub(in crate::runner) fn wait_state(&self) -> Option<&EntryState> {
        self.wait.as_ref().filter(|state| {
            matches!(
                state.phase,
                EntryPhase::Waiting
                    | EntryPhase::Starting
                    | EntryPhase::NotStarted
                    | EntryPhase::Failed
            )
        })
    }

    /// Reads the published wait. A record that is not this entry's own is no
    /// observation, and an entry that started no longer waits.
    fn read_wait(&mut self) {
        self.wait = self.tab.wait_ledger.read(&self.tab.name);
        self.settled = self
            .wait
            .as_ref()
            .is_some_and(|state| !state.holds_a_wait());
    }

    /// This follower's status line, and whether it changed since the last
    /// call. The first call always reports, so a tab opens with its state.
    fn take_status_change(&mut self, now: std::time::Instant) -> Option<HostStatus> {
        let mut changed = None;
        if now >= self.next_poll {
            self.next_poll = now + LISTENER_POLL;
            self.read_wait();
            if let Some(state) = &self.wait {
                let reported = match state.phase {
                    EntryPhase::NotStarted | EntryPhase::Failed => {
                        Some(HostStatus::NotStarted(state.status_line()))
                    }
                    _ if state.waiting_on.is_some() => {
                        Some(HostStatus::Waiting(state.status_line()))
                    }
                    _ => None,
                };
                if let Some(status) = reported {
                    if status != self.status {
                        self.status = status.clone();
                        // A published wait keeps reporting as its elapsed time
                        // advances: the line is live, not a canned label.
                        self.announced = false;
                        return Some(status);
                    }
                    // The wait is unchanged and already announced: no repeat.
                    if self.announced {
                        return None;
                    }
                    self.announced = true;
                    return Some(status);
                }
            }
            if let Some(url) = ready_url(&self.tab) {
                let ready = HostStatus::Ready(url);
                if ready != self.status {
                    changed = Some(ready.clone());
                    self.status = ready;
                }
            }
        }
        if !self.announced || changed.is_some() {
            self.announced = true;
            let status = changed.clone().unwrap_or_else(|| self.status.clone());
            return Some(status);
        }
        None
    }

    /// New complete log lines, each with the status it reports (if any), and
    /// the status changes the listener state file announced.
    pub(in crate::runner) fn poll(&mut self) -> HostPoll {
        let now = std::time::Instant::now();
        let mut poll = HostPoll::default();
        if let Some(status) = self.take_status_change(now) {
            poll.statuses.push(status);
        }
        for line in self.tail.read(&self.tab.log) {
            poll.lines.push(line.clone());
            if let Some(status) = status_from_log_line(&self.tab, &line) {
                if status != self.status {
                    self.status = status.clone();
                    poll.statuses.push(status);
                }
            }
        }
        poll
    }
}

/// One poll of a host process: log lines and status transitions, in order.
#[derive(Debug, Default)]
pub(in crate::runner) struct HostPoll {
    pub(in crate::runner) lines: Vec<String>,
    pub(in crate::runner) statuses: Vec<HostStatus>,
}

/// A bounded incremental reader for one host process's log file. Lines are
/// returned whole; a partial trailing line waits for the next read. A file
/// that shrank (a fresh generation's truncation) is read from its start.
#[derive(Debug, Default)]
struct LogTail {
    offset: u64,
    partial: Vec<u8>,
}

impl LogTail {
    fn new() -> Self {
        Self::default()
    }

    fn read(&mut self, path: &Path) -> Vec<String> {
        use std::io::{Read as _, Seek as _, SeekFrom};

        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(metadata) = file.metadata() else {
            return Vec::new();
        };
        if metadata.len() < self.offset {
            // A new generation opened the log again from the start.
            self.offset = 0;
            self.partial.clear();
        }
        if metadata.len() == self.offset {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut chunk = Vec::new();
        if file.take(READ_MAX as u64).read_to_end(&mut chunk).is_err() {
            return Vec::new();
        }
        self.offset += chunk.len() as u64;
        self.partial.extend_from_slice(&chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line = self.partial.drain(..=end).collect::<Vec<u8>>();
            lines.push(
                String::from_utf8_lossy(&line)
                    .trim_end_matches(['\n', '\r'])
                    .to_owned(),
            );
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(restart: HostProcessRestart, listener_state: Option<PathBuf>) -> HostProcessTab {
        HostProcessTab {
            name: "api".to_owned(),
            log: PathBuf::from("/nonexistent/api.log"),
            listener_state,
            restart,
            restart_delay_ms: 100,
            wait_ledger: crate::runner::managed_wait::WaitLedger::environment(
                Path::new("/nonexistent"),
                "app",
            ),
        }
    }

    #[test]
    fn supervisor_log_lines_report_start_restart_and_exit() {
        let always = tab(HostProcessRestart::Always, None);
        assert_eq!(
            status_from_log_line(&always, "[effigy host-process] started `api` (pid 12)"),
            Some(HostStatus::Running)
        );
        assert_eq!(
            status_from_log_line(&always, "[effigy host-process] `api` exited (code 3)"),
            Some(HostStatus::Restarting)
        );
        let never = tab(HostProcessRestart::Never, None);
        assert_eq!(
            status_from_log_line(&never, "[effigy host-process] `api` exited (code 3)"),
            Some(HostStatus::Exited(3))
        );
        let on_failure = tab(HostProcessRestart::OnFailure, None);
        assert_eq!(
            status_from_log_line(&on_failure, "[effigy host-process] `api` exited (code 0)"),
            Some(HostStatus::Exited(0)),
            "a clean exit is not a failure the policy replaces"
        );
        assert_eq!(
            status_from_log_line(
                &on_failure,
                "[effigy host-process] failed to spawn `x`: nope"
            ),
            Some(HostStatus::Exited(127))
        );
    }

    #[test]
    fn a_programs_own_line_is_never_a_status() {
        let tab = tab(HostProcessRestart::Never, None);
        assert_eq!(status_from_log_line(&tab, "listening on :3000"), None);
        assert_eq!(
            status_from_log_line(&tab, "my app: [effigy host-process] started `x`"),
            None
        );
    }

    #[test]
    fn a_listener_state_file_names_its_ready_url() {
        let dir = tempfile::tempdir().expect("state dir");
        let state = dir.path().join("api.listener.json");
        std::fs::write(&state, r#"{"status":"starting"}"#).expect("starting state");
        let tab = tab(HostProcessRestart::Never, Some(state.clone()));
        assert_eq!(ready_url(&tab), None, "a starting listener is not ready");

        std::fs::write(
            &state,
            r#"{"status":"ready","public_url":"https://api.test","internal_url":"http://127.0.0.1:5000"}"#,
        )
        .expect("ready state");
        assert_eq!(ready_url(&tab).as_deref(), Some("https://api.test"));

        std::fs::write(&state, "{ truncated").expect("torn state");
        assert_eq!(ready_url(&tab), None, "a torn state file is no observation");
    }

    /// A host process that has not started is held by the bring-up, and the
    /// tab says exactly what it waits for; a prerequisite failure is terminal
    /// and attributed (effigy#458).
    #[test]
    fn a_published_wait_names_the_dependency_and_a_failure_is_attributed() {
        use effigy_tui::waits::{Attribution, EntryPhase, EntryState, WaitOn};

        let dir = tempfile::tempdir().expect("wait dir");
        let ledger = crate::runner::managed_wait::WaitLedger::environment(dir.path(), "app");
        ledger.clear();
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.name = "front".to_owned();
        tab.wait_ledger = ledger.clone();

        ledger.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Starting,
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 4_000,
        });
        let mut follower = HostFollower::new(tab.clone());
        assert_eq!(
            follower.poll().statuses,
            vec![HostStatus::Waiting(
                "waiting for api (starting) (4s)".to_owned()
            )]
        );
        assert_eq!(
            follower
                .wait_state()
                .and_then(|state| state.waiting_on.clone()),
            Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Starting,
            })
        );

        ledger.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(Attribution::new("api", "failed (status 500)")),
            detail: None,
            elapsed_ms: 0,
        });
        // The poll interval must pass before the record is re-read.
        std::thread::sleep(ListenerPollWait::value());
        assert_eq!(
            follower.poll().statuses,
            vec![HostStatus::NotStarted(
                "not started — api failed (status 500)".to_owned()
            )]
        );
    }

    /// The poll interval, as a duration a test may wait.
    struct ListenerPollWait;

    impl ListenerPollWait {
        fn value() -> std::time::Duration {
            LISTENER_POLL + std::time::Duration::from_millis(20)
        }
    }

    /// A settled record is not a wait: once the listener is ready the owner
    /// records that it settled, and the tab stops naming the probe
    /// (effigy#458).
    #[test]
    fn a_settled_record_names_no_wait() {
        let dir = tempfile::tempdir().expect("wait dir");
        let ledger = crate::runner::managed_wait::WaitLedger::environment(dir.path(), "app");
        ledger.clear();
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.wait_ledger = ledger.clone();
        ledger.publish(&effigy_tui::waits::EntryState {
            entry: "api".to_owned(),
            phase: effigy_tui::waits::EntryPhase::Starting,
            waiting_on: Some(effigy_tui::waits::WaitOn::Probe {
                probe: "GET / -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 12_000,
        });
        let mut follower = HostFollower::new(tab.clone());
        assert_eq!(
            follower.poll().statuses,
            vec![HostStatus::Waiting(
                "waiting for GET / -> 200 (12s)".to_owned()
            )]
        );
        // The entry settled: the bring-up records that, and the tab stops
        // naming the probe.
        ledger.settle("api");
        std::thread::sleep(ListenerPollWait::value());
        follower.poll();
        assert_eq!(
            follower.wait_state(),
            None,
            "a record that no longer says the process is waiting is no wait"
        );
    }

    /// The handoff window: while the planner's snapshot is staged but not yet
    /// committed, a tab reads the run's complete environment wait and never
    /// announces a start it has no evidence for; once the planner commits, it
    /// names the planner's state; only a settled record or the supervisor's own
    /// report announces the process (effigy#458).
    #[test]
    fn a_staged_snapshot_never_erases_a_wait_or_announces_a_start() {
        use effigy_tui::waits::{EntryPhase, EntryState, WaitOn};

        let dir = tempfile::tempdir().expect("wait dir");
        let ledger = crate::runner::managed_wait::WaitLedger::environment(dir.path(), "app");
        ledger.clear();
        let parent = ledger.stage(crate::runner::managed_wait::WaitOwner::Run);
        parent.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        ledger.commit(&parent);

        let mut tab = tab(HostProcessRestart::Never, None);
        tab.wait_ledger = ledger.clone();
        let mut follower = HostFollower::new(tab.clone());
        follower.poll();
        assert_eq!(
            follower
                .wait_state()
                .map(|state| state.tab_label())
                .as_deref(),
            Some("api [waiting: environment]")
        );
        assert!(!follower.may_announce_started());

        // The planner stages its own snapshot, not visible yet.
        let planner = ledger.stage(crate::runner::managed_wait::WaitOwner::Planner);
        planner.publish(&EntryState {
            entry: "api".to_owned(),
            phase: EntryPhase::Starting,
            waiting_on: Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        ledger.commit(&planner);
        std::thread::sleep(ListenerPollWait::value());
        follower.poll();
        assert_eq!(
            follower
                .wait_state()
                .map(|state| state.tab_label())
                .as_deref(),
            Some("api [starting: GET / -> 200 (0s)]"),
            "the committed planner snapshot is the one read"
        );
        assert!(
            !follower.may_announce_started(),
            "a probe wait is not a start"
        );

        // The planner launched it and settled the wait.
        planner.settle("api");
        std::thread::sleep(ListenerPollWait::value());
        follower.poll();
        assert!(
            follower.may_announce_started(),
            "a settled record is evidence the process started"
        );
    }

    /// A late TUI attach reads the record named by its own tab, even when two
    /// accepted names differ only by a character a lossy file name would have
    /// folded together (effigy#458).
    #[test]
    fn a_late_attach_reads_the_entry_its_tab_names() {
        use effigy_tui::waits::{EntryPhase, EntryState, WaitOn};

        let dir = tempfile::tempdir().expect("wait dir");
        let ledger = crate::runner::managed_wait::WaitLedger::environment(dir.path(), "app");
        ledger.clear();
        for (entry, probe) in [("api v1", "GET / -> 200"), ("api-v1", "GET /health -> 204")] {
            ledger.publish(&EntryState {
                entry: entry.to_owned(),
                phase: EntryPhase::Starting,
                waiting_on: Some(WaitOn::Probe {
                    probe: probe.to_owned(),
                }),
                not_started_because: None,
                detail: None,
                elapsed_ms: 0,
            });
        }
        let mut spaced = tab(HostProcessRestart::Never, None);
        spaced.name = "api v1".to_owned();
        spaced.wait_ledger = ledger.clone();
        let mut dashed = tab(HostProcessRestart::Never, None);
        dashed.name = "api-v1".to_owned();
        dashed.wait_ledger = ledger.clone();

        assert_tab_reads_probe(&mut spaced, "GET / -> 200");
        assert_tab_reads_probe(&mut dashed, "GET /health -> 204");
    }

    fn assert_tab_reads_probe(tab: &mut HostProcessTab, probe: &str) {
        let mut follower = HostFollower::new(tab.clone());
        follower.poll();
        assert_eq!(
            follower
                .wait_state()
                .and_then(|state| state.waiting_on.clone()),
            Some(effigy_tui::waits::WaitOn::Probe {
                probe: probe.to_owned()
            }),
            "the tab reads its own entry's wait"
        );
    }

    /// A host that is still waiting for the environment (or for a reader) is    /// A host that is still waiting for the environment (or for a reader) is
    /// never announced as started, so its tab cannot claim it ran; a process
    /// with no wait is (effigy#458).
    #[test]
    fn a_blocked_host_is_not_announced_as_started() {
        use effigy_tui::waits::{EntryPhase, EntryState, WaitOn};

        let dir = tempfile::tempdir().expect("wait dir");
        let ledger = crate::runner::managed_wait::WaitLedger::environment(dir.path(), "app");
        ledger.clear();
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.name = "front".to_owned();
        tab.wait_ledger = ledger.clone();

        ledger.publish(&EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        let mut follower = HostFollower::new(tab.clone());
        assert_eq!(
            follower.poll().statuses,
            vec![HostStatus::Waiting(
                "waiting for environment (containers 3/4 healthy) (0s)".to_owned()
            )]
        );
        assert!(
            !follower.may_announce_started(),
            "a host that waits for the environment has not started"
        );

        // The planner launched it and settled its wait.
        ledger.settle("front");
        std::thread::sleep(ListenerPollWait::value());
        follower.poll();
        assert!(
            follower.may_announce_started(),
            "a process with no wait is the one to announce"
        );
    }

    #[test]
    fn a_log_tail_returns_whole_lines_and_survives_a_truncation() {
        let dir = tempfile::tempdir().expect("log dir");
        let log = dir.path().join("api.log");
        std::fs::write(&log, "one\ntwo\npart").expect("first write");
        let mut tail = LogTail::new();
        assert_eq!(
            tail.read(&log),
            vec!["one".to_owned(), "two".to_owned()],
            "a partial trailing line waits"
        );

        std::fs::write(&log, "one\ntwo\npartial line\nthree\n").expect("append");
        assert_eq!(
            tail.read(&log),
            vec!["partial line".to_owned(), "three".to_owned()],
            "the partial line completes before it is shown"
        );

        std::fs::write(&log, "fresh\n").expect("truncate for a new generation");
        assert_eq!(tail.read(&log), vec!["fresh".to_owned()]);
    }
}
