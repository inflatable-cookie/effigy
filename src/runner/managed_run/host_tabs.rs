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
            Self::Ready(url) => format!("ready on {url}"),
            Self::Restarting => "restarting".to_owned(),
            Self::Exited(code) => format!("exited {code}"),
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
        // Reject legacy paths before opening payload bytes. Identity is checked
        // again after the bounded read so a raced replacement publishes none.
        let Ok(before) = effigy_kernel::custody::MergedLog::identity(&self.tab.log) else {
            if !self.log_refused {
                poll.statuses.push(HostStatus::LogUnavailable);
            }
            self.log_refused = true;
            return poll;
        };
        if std::mem::take(&mut self.log_refused) {
            poll.statuses.push(self.status.clone());
        }
        let was_failed = self.tail.failed;
        let lines = self
            .tail
            .read_verified(&self.tab.log, Some(before.identity));
        if !effigy_kernel::custody::MergedLog::identity(&self.tab.log)
            .is_ok_and(|after| after.identity == before.identity)
        {
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
        for line in lines {
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
            log: PathBuf::from("/nonexistent/api.log"),
            listener_state,
            restart,
            restart_delay_ms: 100,
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
