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
//!   listener, the old reader path for an already-running older supervisor;
//! - the status line, derived from the owner journal and a generation-bound
//!   lifecycle observation, never child log text or a process-tree scrape;
//! - the shared output cursor, a bounded incremental reader for the log file, shared by the
//!   TUI tabs and the `--headless` front end so both show the same lines.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::runner::host_process::{HostLifecycleStatus, HostPaneIdentity, HostPaneObservation};
use effigy_containers::{EffectiveContainerPolicy, HostProcessRestart};
use effigy_core::managed_waits::{EntryPhase, EntryState};

/// The most one log tail read returns, so a chatty process cannot stall a
/// frame or a headless turn.
use super::output::Cursor;
/// How often a front end reads the host owner's lifecycle observation.
const LISTENER_POLL: Duration = Duration::from_millis(250);

/// One environment host process, as a tab and as a followed log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessTab {
    pub(in crate::runner) name: String,
    pub(in crate::runner) identity: Option<HostPaneIdentity>,
    pub(in crate::runner) log: PathBuf,
    /// The legacy reader path; new owners publish lifecycle observations in the journal.
    pub(in crate::runner) listener_state: Option<PathBuf>,
    pub(in crate::runner) restart: HostProcessRestart,
    pub(in crate::runner) restart_delay_ms: u64,
    /// The activation observations for this environment. Attached clients bind
    /// this reader to their exact parent run before polling it.
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
                identity: HostPaneIdentity::new(repo_root, policy, &process.name),
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
    LogReplaced,
    LogUnavailable,
    LogTruncated,
    /// A generation is running; a listener that is not serving yet waits here.
    Running,
    Unavailable,
    Failed,
    Stopped,
    Stopping,
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
            Self::LogUnavailable => "output unavailable: canonical provenance is missing or changed; restart the host owner, then reopen this follower".to_owned(),
            Self::LogReplaced => "output log replaced; reopened at byte zero (reopen older attached followers)".to_owned(),
            Self::LogTruncated => "output follow stopped: invalid text or line buffer exceeded; inspect the canonical log before reopening".to_owned(),
            Self::Running => "running".to_owned(),
            Self::Unavailable => "unavailable: no current ready owner generation".to_owned(),
            Self::Failed => "failed".to_owned(),
            Self::Stopped => "stopped".to_owned(),
            Self::Stopping => "stopping".to_owned(),
            Self::Ready(url) => format!("ready on {url}"),
            Self::Restarting => "restarting".to_owned(),
            Self::Exited(code) => format!("exited {code}"),
            Self::Waiting(what) => what.clone(),
            Self::NotStarted(what) => what.clone(),
        }
    }
}

/// Only typed owner/journal observations can change a pane's status.
fn observed_status(tab: &HostProcessTab, view: &HostPaneObservation) -> HostStatus {
    use effigy_kernel::run::Phase;
    if matches!(view.lifecycle, Some((HostLifecycleStatus::Failed, _))) {
        return HostStatus::Failed;
    }
    if view.phase == Phase::Closed {
        return view
            .exit
            .map(HostStatus::Exited)
            .unwrap_or(HostStatus::Stopped);
    }
    if view.phase == Phase::Stopping {
        return HostStatus::Stopping;
    }
    if !view.live {
        return HostStatus::Unavailable;
    }
    if let Some(code) = view.exit {
        return if tab.restarts_after(code) {
            HostStatus::Restarting
        } else {
            HostStatus::Exited(code)
        };
    }
    if let Some((status, url)) = &view.lifecycle {
        match status {
            HostLifecycleStatus::Ready => {
                return url
                    .clone()
                    .map(HostStatus::Ready)
                    .unwrap_or(HostStatus::Unavailable)
            }
            HostLifecycleStatus::Unavailable | HostLifecycleStatus::OwnerGone => {
                return HostStatus::Unavailable
            }
            HostLifecycleStatus::Failed => return HostStatus::Failed,
            HostLifecycleStatus::Stopped => return HostStatus::Stopped,
            HostLifecycleStatus::Starting => return HostStatus::Starting,
            HostLifecycleStatus::Running => return HostStatus::Running,
        }
    }
    if matches!(view.phase, Phase::Admitted | Phase::Starting) {
        HostStatus::Starting
    } else {
        HostStatus::Running
    }
}

/// One host process's followed log and status, as a front end renders it.
#[derive(Debug)]
pub(in crate::runner) struct HostFollower {
    pub(in crate::runner) tab: HostProcessTab,
    tail: Cursor,
    status: HostStatus,
    announced: bool,
    log_refused: bool,
    /// Whether the tab has been told this process exists, so its startup
    /// projection stops waiting for it.
    started: bool,
    next_poll: std::time::Instant,
    /// The bring-up's published wait, while it holds this process.
    wait: Option<EntryState>,
    /// Whether this process has been observed at all: one of its own published
    /// wait records, a settled launch or a typed owner observation. Before
    /// the first observation the tab reports no status, so no front end claims
    /// a process is starting while the bring-up has not reached it
    /// (effigy#458).
    evidence: bool,
}

impl HostFollower {
    pub(in crate::runner) fn new(tab: HostProcessTab) -> Self {
        Self {
            tab,
            tail: Cursor::default(),
            status: HostStatus::Starting,
            announced: false,
            log_refused: false,
            started: false,
            next_poll: std::time::Instant::now(),
            wait: None,
            evidence: false,
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
    /// window before an activation publishes its observation, is no evidence, so a tab
    /// never claims a process ran because its wait is not visible yet
    /// (effigy#458).
    pub(in crate::runner) fn may_announce_started(&self) -> bool {
        self.wait_state().is_none() && (self.wait.is_some() || self.status.reports_a_start())
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

    /// This follower's status line, and whether it changed since the last
    /// call. The first observed state reports; no record is not a start.
    fn take_status_change(&mut self, now: std::time::Instant) -> Option<HostStatus> {
        if now < self.next_poll {
            return None;
        }
        self.next_poll = now + LISTENER_POLL;
        self.wait = self.tab.wait_ledger.read(&self.tab.name);
        self.evidence |= self.wait.is_some();
        let waiting = self.wait.as_ref().and_then(|state| match state.phase {
            EntryPhase::NotStarted | EntryPhase::Failed => {
                Some(HostStatus::NotStarted(state.status_line()))
            }
            _ if state.waiting_on.is_some() => Some(HostStatus::Waiting(state.status_line())),
            _ => None,
        });
        let status = waiting.unwrap_or_else(|| {
            let observed = self
                .tab
                .identity
                .as_ref()
                .and_then(|identity| identity.read(self.tab.listener_state.as_deref()));
            self.evidence |= observed.is_some();
            observed
                .map(|view| observed_status(&self.tab, &view))
                .unwrap_or(HostStatus::Unavailable)
        });
        let changed = status != self.status;
        self.status = status;
        if !self.evidence || (self.announced && !changed) {
            return None;
        }
        self.announced = true;
        Some(self.status.clone())
    }

    /// New complete log lines, each with the status it reports (if any), and
    /// the status changes the owner observation announced.
    pub(in crate::runner) fn poll(&mut self) -> HostPoll {
        let now = std::time::Instant::now();
        let mut poll = HostPoll::default();
        if let Some(status) = self.take_status_change(now) {
            poll.statuses.push(status);
        }
        let (mut output, chunk) = match self.tail.host(&self.tab.log) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return poll,
            Err(_) => {
                if !self.evidence || self.wait_state().is_some() {
                    return poll;
                }
                if !self.log_refused {
                    poll.statuses.push(HostStatus::LogUnavailable);
                }
                self.log_refused = true;
                return poll;
            }
        };
        if std::mem::take(&mut self.log_refused) {
            poll.statuses.push(self.status.clone());
        }
        let finish = matches!(
            self.status,
            HostStatus::Exited(_) | HostStatus::Stopped | HostStatus::Failed
        ) && output.caught_up;
        poll.lines = self.tail.lines(&mut output, finish);
        if !output.text.is_empty() {
            poll.styled.push(chunk);
        }
        if output.replaced {
            poll.statuses.push(HostStatus::LogReplaced);
        }
        if output.failed {
            poll.statuses.push(HostStatus::LogTruncated);
        }
        poll
    }
}

/// One poll of a host process: log lines and status transitions, in order.
#[derive(Debug, Default)]
pub(in crate::runner) struct HostPoll {
    pub(in crate::runner) lines: Vec<String>,
    pub(in crate::runner) statuses: Vec<HostStatus>,
    pub(in crate::runner) styled: Vec<effigy_core::terminal_text::StyledChunk>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(entry: &str) -> EntryState {
        EntryState {
            entry: entry.to_owned(),
            phase: EntryPhase::Ready,
            waiting_on: None,
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        }
    }

    fn tab(restart: HostProcessRestart, listener_state: Option<PathBuf>) -> HostProcessTab {
        HostProcessTab {
            name: "api".to_owned(),
            identity: None,
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
    fn typed_owner_status_clears_ready_for_unavailable_and_terminal_states() {
        let tab = tab(HostProcessRestart::OnFailure, None);
        let mut view = HostPaneObservation {
            phase: effigy_kernel::run::Phase::Active,
            live: true,
            exit: None,
            lifecycle: Some((HostLifecycleStatus::Ready, Some("https://api.test".into()))),
        };
        assert_eq!(
            observed_status(&tab, &view),
            HostStatus::Ready("https://api.test".into())
        );
        view.lifecycle = Some((HostLifecycleStatus::Unavailable, None));
        assert_eq!(observed_status(&tab, &view), HostStatus::Unavailable);
        view.lifecycle = Some((HostLifecycleStatus::Ready, Some("https://api.test".into())));
        assert_eq!(
            observed_status(&tab, &view),
            HostStatus::Ready("https://api.test".into())
        );
        for (state, status) in [
            (HostLifecycleStatus::Failed, HostStatus::Failed),
            (HostLifecycleStatus::Stopped, HostStatus::Stopped),
            (HostLifecycleStatus::OwnerGone, HostStatus::Unavailable),
        ] {
            view.lifecycle = Some((state, None));
            assert_eq!(observed_status(&tab, &view), status);
        }
        view.lifecycle = None;
        assert_eq!(observed_status(&tab, &view), HostStatus::Running);
    }
    #[test]
    fn a_journaled_launch_refusal_never_announces_a_start_without_an_event() {
        let tab = tab(HostProcessRestart::Never, None);
        let view = HostPaneObservation {
            phase: effigy_kernel::run::Phase::Closed,
            live: false,
            exit: Some(127),
            lifecycle: Some((HostLifecycleStatus::Failed, None)),
        };
        let status = observed_status(&tab, &view);
        assert_eq!(status, HostStatus::Failed);
        assert!(!status.reports_a_start());
        let mut follower = HostFollower::new(tab);
        follower.status = status;
        assert!(!follower.may_announce_started());
    }

    #[test]
    fn child_owner_marker_spoofs_never_change_status() {
        let mut follower = HostFollower::new(tab(HostProcessRestart::Never, None));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spoof.log");
        std::fs::write(&path, "[effigy host-process] `api` exited (code 0)\n").unwrap();
        // Log parsing returns display bytes only; lifecycle observation has no
        // log-text input, including a forged marker at the beginning of a line.
        assert_eq!(
            follower.tail.read(&path),
            ["[effigy host-process] `api` exited (code 0)"]
        );
        assert_eq!(follower.status, HostStatus::Starting);
    }

    /// No observation is not a start: a host the bring-up has not reached yet
    /// reports no status at all, so neither the headless text nor a tab can
    /// claim it is starting before the planner launched it (effigy#458).
    #[test]
    fn an_unobserved_host_process_reports_no_starting_status() {
        let dir = tempfile::tempdir().expect("wait dir");
        let publisher =
            crate::runner::managed_wait::WaitPublisher::fixture(dir.path(), Some("app"));
        let ledger = publisher.environment_source(dir.path(), "app");
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.name = "api".to_owned();
        tab.wait_ledger = ledger.clone();
        let mut follower = HostFollower::new(tab);

        // The pre-publication window: nothing has been said about this host.
        assert!(
            follower.poll().statuses.is_empty(),
            "an unobserved host is not reported as starting"
        );
        assert!(follower.poll().statuses.is_empty());
        assert!(
            !follower.may_announce_started(),
            "and it may not be announced as started"
        );
    }

    /// A host process that has not started is held by the bring-up, and the
    /// tab says exactly what it waits for; a prerequisite failure is terminal
    /// and attributed (effigy#458).
    #[test]
    fn a_published_wait_names_the_dependency_and_a_failure_is_attributed() {
        use effigy_core::managed_waits::{Attribution, EntryPhase, EntryState, WaitOn};

        let dir = tempfile::tempdir().expect("wait dir");
        let publisher =
            crate::runner::managed_wait::WaitPublisher::fixture(dir.path(), Some("app"));
        let ledger = publisher.environment_source(dir.path(), "app");
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.name = "front".to_owned();
        tab.wait_ledger = ledger.clone();

        publisher.publish_hosts(&[EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Starting,
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 4_000,
        }]);
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

        assert!(
            follower.poll().statuses.is_empty(),
            "no repeat before the poll interval"
        );
        std::thread::sleep(ListenerPollWait::value());
        assert!(
            follower.poll().statuses.is_empty(),
            "an unchanged wait is announced once"
        );

        publisher.publish_hosts(&[EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(Attribution::new("api", "failed (status 500)")),
            detail: None,
            elapsed_ms: 0,
        }]);
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
        let publisher =
            crate::runner::managed_wait::WaitPublisher::fixture(dir.path(), Some("app"));
        let ledger = publisher.environment_source(dir.path(), "app");
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.wait_ledger = ledger.clone();
        publisher.publish_hosts(&[effigy_core::managed_waits::EntryState {
            entry: "api".to_owned(),
            phase: effigy_core::managed_waits::EntryPhase::Starting,
            waiting_on: Some(effigy_core::managed_waits::WaitOn::Probe {
                probe: "GET / -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 12_000,
        }]);
        let mut follower = HostFollower::new(tab.clone());
        assert_eq!(
            follower.poll().statuses,
            vec![HostStatus::Waiting(
                "waiting for GET / -> 200 (12s)".to_owned()
            )]
        );
        // The entry settled: the bring-up records that, and the tab stops
        // naming the probe.
        publisher.publish_hosts(&[settled("api")]);
        std::thread::sleep(ListenerPollWait::value());
        follower.poll();
        assert_eq!(
            follower.wait_state(),
            None,
            "a record that no longer says the process is waiting is no wait"
        );
    }

    /// A late TUI attach reads the record named by its own tab, even when two
    /// accepted names differ only by a character a lossy file name would have
    /// folded together (effigy#458).
    #[test]
    fn a_late_attach_reads_the_entry_its_tab_names() {
        use effigy_core::managed_waits::WaitOn;
        let dir = tempfile::tempdir().expect("wait dir");
        let publisher =
            crate::runner::managed_wait::WaitPublisher::fixture(dir.path(), Some("app"));
        let ledger = publisher.environment_source(dir.path(), "app");
        let cases = [("api v1", "GET / -> 200"), ("api-v1", "GET /health -> 204")];
        publisher.publish_hosts(&cases.map(|(entry, probe)| EntryState {
            phase: EntryPhase::Starting,
            waiting_on: Some(WaitOn::Probe {
                probe: probe.to_owned(),
            }),
            ..settled(entry)
        }));
        for (entry, probe) in cases {
            let mut tab = tab(HostProcessRestart::Never, None);
            tab.name = entry.to_owned();
            tab.wait_ledger = ledger.clone();
            let mut follower = HostFollower::new(tab);
            follower.poll();
            assert_eq!(
                follower
                    .wait_state()
                    .and_then(|state| state.waiting_on.clone()),
                Some(WaitOn::Probe {
                    probe: probe.to_owned()
                }),
                "the tab reads its own entry's wait"
            );
        }
    }

    /// A host that is still waiting for the environment (or for a reader) is
    /// never announced as started, so its tab cannot claim it ran; a process
    /// with no wait is (effigy#458).
    #[test]
    fn a_blocked_host_is_not_announced_as_started() {
        use effigy_core::managed_waits::{EntryPhase, EntryState, WaitOn};

        let dir = tempfile::tempdir().expect("wait dir");
        let publisher =
            crate::runner::managed_wait::WaitPublisher::fixture(dir.path(), Some("app"));
        let ledger = publisher.environment_source(dir.path(), "app");
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.name = "front".to_owned();
        tab.wait_ledger = ledger.clone();

        publisher.publish_hosts(&[EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        }]);
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
        publisher.publish_hosts(&[settled("front")]);
        std::thread::sleep(ListenerPollWait::value());
        follower.poll();
        assert!(
            follower.may_announce_started(),
            "a process with no wait is the one to announce"
        );
    }
}
