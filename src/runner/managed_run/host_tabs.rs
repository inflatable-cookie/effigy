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
//! - the status line, derived from the owner journal and a generation-bound
//!   listener record, never child log text or a process-tree scrape;
//! - [`LogTail`], a bounded incremental reader for the log file, shared by the
//!   TUI tabs and the `--headless` front end so both show the same lines.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::runner::host_process::{HostPaneIdentity, HostPaneObservation};
use effigy_containers::{EffectiveContainerPolicy, HostProcessRestart};

/// The most one log tail read returns, so a chatty process cannot stall a
/// frame or a headless turn.
const READ_MAX: usize = 64 * 1024;
/// How often a front end asks a listener's state file whether it is ready.
const LISTENER_POLL: Duration = Duration::from_millis(250);

/// One environment host process, as a tab and as a followed log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct HostProcessTab {
    pub(in crate::runner) name: String,
    pub(in crate::runner) identity: Option<HostPaneIdentity>,
    pub(in crate::runner) log: PathBuf,
    /// The state file a listener publishes; `None` for a plain process.
    pub(in crate::runner) listener_state: Option<PathBuf>,
    pub(in crate::runner) restart: HostProcessRestart,
    pub(in crate::runner) restart_delay_ms: u64,
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
}

impl HostStatus {
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
        }
    }
}

/// Only typed owner/journal observations can change a pane's status.
fn observed_status(tab: &HostProcessTab, view: &HostPaneObservation) -> HostStatus {
    use effigy_kernel::run::Phase;
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
    if let Some((status, url)) = &view.listener {
        match status.as_str() {
            "ready" => {
                return url
                    .clone()
                    .map(HostStatus::Ready)
                    .unwrap_or(HostStatus::Unavailable)
            }
            "unavailable" => return HostStatus::Unavailable,
            "failed" => return HostStatus::Failed,
            "stopped" => return HostStatus::Stopped,
            "starting" => return HostStatus::Starting,
            _ => {}
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
    tail: LogTail,
    status: HostStatus,
    announced: bool,
    log_refused: bool,
    /// Whether the tab has been told this process exists, so its startup
    /// projection stops waiting for it.
    started: bool,
    next_poll: std::time::Instant,
}

impl HostFollower {
    pub(in crate::runner) fn new(tab: HostProcessTab) -> Self {
        Self {
            tab,
            tail: LogTail::new(),
            status: HostStatus::Starting,
            announced: false,
            log_refused: false,
            started: false,
            next_poll: std::time::Instant::now(),
        }
    }

    /// Marks this host process as announced to the session. `true` when it
    /// already was.
    pub(in crate::runner) fn announce_started(&mut self) -> bool {
        std::mem::replace(&mut self.started, true)
    }

    /// This follower's status line, and whether it changed since the last
    /// call. The first call always reports, so a tab opens with its state.
    fn take_status_change(&mut self, now: std::time::Instant) -> Option<HostStatus> {
        let mut changed = None;
        if now >= self.next_poll {
            self.next_poll = now + LISTENER_POLL;
            let status = self
                .tab
                .identity
                .as_ref()
                .and_then(|identity| identity.read(self.tab.listener_state.as_deref()))
                .map(|view| observed_status(&self.tab, &view))
                .unwrap_or(HostStatus::Unavailable);
            if status != self.status {
                changed = Some(status.clone());
                self.status = status;
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
        // Reject legacy paths before opening payload bytes. Identity is checked
        // again after the bounded read so a raced replacement publishes none.
        let before = match effigy_kernel::custody::MergedLog::identity(&self.tab.log) {
            Ok(before) => before,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return poll,
            Err(_) => {
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
        let was_failed = self.tail.failed;
        let lines = self
            .tail
            .read_verified(&self.tab.log, Some(before.identity));
        let same = match effigy_kernel::custody::MergedLog::identity(&self.tab.log) {
            Ok(after) => after.identity == before.identity,
            // read_verified already checked the committed range and inode.
            // A new append cannot invalidate that prefix or reset its cursor.
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => true,
            Err(_) => false,
        };
        if !same {
            self.tail = LogTail::new();
            poll.statuses.push(HostStatus::LogReplaced);
            return poll;
        }
        poll.styled = std::mem::take(&mut self.tail.styled);
        if self.tail.replaced {
            poll.statuses.push(HostStatus::LogReplaced);
        }
        if self.tail.failed && !was_failed {
            poll.statuses.push(HostStatus::LogTruncated);
        }
        poll.lines = lines;
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

/// A bounded incremental reader for one host process's log file. Lines are
/// returned whole; a partial trailing line waits for the next read. A file
/// that shrank (a fresh generation's truncation) is read from its start.
#[derive(Debug, Default)]
struct LogTail {
    offset: u64,
    identity: Option<(u64, u64)>,
    replaced: bool,
    failed: bool,
    partial: Vec<u8>,
    styled: Vec<effigy_core::terminal_text::StyledChunk>,
}

impl LogTail {
    fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn read(&mut self, path: &Path) -> Vec<String> {
        self.read_verified(path, None)
    }

    fn read_verified(
        &mut self,
        path: &Path,
        expected: Option<effigy_kernel::custody::FileIdentity>,
    ) -> Vec<String> {
        use std::io::{Read as _, Seek as _, SeekFrom};

        use std::os::unix::fs::OpenOptionsExt as _;
        let Ok(mut file) = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        else {
            return Vec::new();
        };
        let Ok(metadata) = file.metadata() else {
            return Vec::new();
        };
        use std::os::unix::fs::MetadataExt as _;
        let identity = (metadata.dev(), metadata.ino());
        if !metadata.is_file() || expected.is_some_and(|id| (id.device, id.inode) != identity) {
            return Vec::new();
        }
        self.replaced =
            self.identity.is_some_and(|prior| prior != identity) || metadata.len() < self.offset;
        if self.replaced {
            self.offset = 0;
            self.partial.clear();
            self.failed = false;
        }
        self.identity = Some(identity);
        if self.failed {
            return Vec::new();
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
        if let Some(id) = expected {
            // Re-open through the provenance gate; exact range matching avoids
            // exposing a raced legacy/replacement descriptor or stale styles.
            let Ok(read) = effigy_kernel::custody::MergedLog::read_plain(
                path,
                Some(id),
                self.offset,
                chunk.len(),
            ) else {
                return Vec::new();
            };
            if read.cutover || !chunk.starts_with(&read.bytes) {
                return Vec::new();
            }
            chunk = read.bytes.clone();
            let spans = effigy_kernel::custody::MergedLog::styles(path, &read);
            self.styled.push(effigy_core::terminal_text::StyledChunk {
                offset: self.offset,
                spans,
            });
        }
        self.offset += chunk.len() as u64;
        let mut lines = Vec::new();
        for byte in chunk {
            if self.partial.len() == READ_MAX {
                self.partial.clear();
                self.failed = true;
                return Vec::new();
            }
            self.partial.push(byte);
            if byte == b'\n' {
                let Ok(line) = String::from_utf8(std::mem::take(&mut self.partial)) else {
                    self.failed = true;
                    return Vec::new();
                };
                if line.chars().any(|ch| ch.is_control() && ch != '\n') {
                    self.failed = true;
                    return Vec::new();
                }
                lines.push(line.trim_end_matches('\n').to_owned());
            }
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
            identity: None,
            log: PathBuf::from("/nonexistent/api.log"),
            listener_state,
            restart,
            restart_delay_ms: 100,
        }
    }

    #[test]
    fn typed_owner_status_clears_ready_for_unavailable_and_terminal_states() {
        let tab = tab(HostProcessRestart::OnFailure, None);
        let mut view = HostPaneObservation {
            phase: effigy_kernel::run::Phase::Active,
            live: true,
            exit: None,
            listener: Some(("ready".into(), Some("https://api.test".into()))),
        };
        assert_eq!(
            observed_status(&tab, &view),
            HostStatus::Ready("https://api.test".into())
        );
        view.listener = Some(("unavailable".into(), None));
        assert_eq!(observed_status(&tab, &view), HostStatus::Unavailable);
        view.listener = Some(("ready".into(), Some("https://api.test".into())));
        assert_eq!(
            observed_status(&tab, &view),
            HostStatus::Ready("https://api.test".into())
        );
        for (state, status) in [
            ("failed", HostStatus::Failed),
            ("stopped", HostStatus::Stopped),
        ] {
            view.listener = Some((state.into(), None));
            assert_eq!(observed_status(&tab, &view), status);
        }
        view.listener = None;
        assert_eq!(observed_status(&tab, &view), HostStatus::Running);
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
    #[test]
    fn larger_replacement_resets_partial_text_and_offset() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("api.log");
        std::fs::write(&log, "old\npartial").unwrap();
        let mut tail = LogTail::new();
        assert_eq!(tail.read(&log), ["old"]);
        let replacement = dir.path().join("new");
        std::fs::write(&replacement, "safe replacement grows beyond old offset\n").unwrap();
        std::fs::rename(replacement, &log).unwrap();
        assert_eq!(
            tail.read(&log),
            ["safe replacement grows beyond old offset"]
        );
        assert!(tail.replaced);
        assert!(tail.partial.is_empty());
    }
    #[test]
    fn huge_line_is_a_bounded_explicit_follow_failure() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("api.log");
        std::fs::write(&log, vec![b'x'; READ_MAX + 1]).unwrap();
        let mut tail = LogTail::new();
        assert!(tail.read(&log).is_empty());
        assert!(tail.read(&log).is_empty());
        assert!(tail.failed);
        assert!(tail.partial.len() <= READ_MAX);
    }
}
