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
        for line in self.tail.read(&self.tab.log) {
            poll.lines.push(line.clone());
            if let Some(status) = status_from_log_line(&self.tab, &String::from_utf8_lossy(&line)) {
                if status != self.status {
                    self.status = status.clone();
                    poll.statuses.push(status);
                }
            }
        }
        poll.generation_changed = self.tail.generation_changed;
        poll.projection_unavailable = self.tail.projection_unavailable;
        poll
    }
}

/// One poll of a host process: log lines and status transitions, in order.
#[derive(Debug, Default)]
pub(in crate::runner) struct HostPoll {
    pub(in crate::runner) lines: Vec<Vec<u8>>,
    pub(in crate::runner) statuses: Vec<HostStatus>,
    pub(in crate::runner) generation_changed: bool,
    pub(in crate::runner) projection_unavailable: bool,
}

/// A bounded incremental reader for one host process's log file. Lines are
/// returned whole; a partial trailing line waits for the next read. A file
/// that shrank (a fresh generation's truncation) is read from its start.
#[derive(Debug, Default)]
struct LogTail {
    offset: u64,
    partial: Vec<u8>,
    generation_changed: bool,
    projection_unavailable: bool,
    identity: Option<(u64, u64)>,
}

impl LogTail {
    fn new() -> Self {
        Self::default()
    }

    fn read(&mut self, path: &Path) -> Vec<Vec<u8>> {
        use std::io::{Read as _, Seek as _, SeekFrom};
        use std::os::unix::fs::MetadataExt;

        self.generation_changed = false;
        self.projection_unavailable = false;
        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(metadata) = file.metadata() else {
            return Vec::new();
        };
        let identity = (metadata.dev(), metadata.ino());
        if self.identity.is_some_and(|previous| previous != identity)
            || metadata.len() < self.offset
        {
            // A new generation opened the log again from the start.
            self.offset = 0;
            self.partial.clear();
            self.generation_changed = true;
        }
        self.identity = Some(identity);
        if metadata.len() == self.offset {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut chunk = Vec::new();
        if (&mut file)
            .take(READ_MAX as u64)
            .read_to_end(&mut chunk)
            .is_err()
        {
            return Vec::new();
        }
        let stable = file
            .metadata()
            .ok()
            .zip(std::fs::symlink_metadata(path).ok())
            .is_some_and(|(after, path_after)| {
                crate::runner::host_process::same_log_snapshot(&metadata, &after)
                    && crate::runner::host_process::same_log_snapshot(&metadata, &path_after)
            });
        if !stable {
            self.projection_unavailable = true;
            return Vec::new();
        }
        self.offset += chunk.len() as u64;
        self.partial.extend_from_slice(&chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let mut line = self.partial.drain(..=end).collect::<Vec<u8>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(line);
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
            vec![b"one".to_vec(), b"two".to_vec()],
            "a partial trailing line waits"
        );

        std::fs::write(&log, "one\ntwo\npartial line\nthree\n").expect("append");
        assert_eq!(
            tail.read(&log),
            vec![b"partial line".to_vec(), b"three".to_vec()],
            "the partial line completes before it is shown"
        );

        std::fs::write(&log, "fresh\n").expect("truncate for a new generation");
        assert_eq!(tail.read(&log), vec![b"fresh".to_vec()]);
    }

    #[test]
    fn underlay_follow_log_keeps_tui_colour_and_plain_reader_strips_it() {
        let dir = tempfile::tempdir().expect("log dir");
        let log = dir.path().join("api.log");
        std::fs::write(
            &log,
            b"\x1b[32mCompiling\x1b[0m api\r\x1b[31merror\x1b[0m\n",
        )
        .expect("log");
        let mut follower = LogTail::new();
        let lines = follower.read(&log);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].windows(5).any(|bytes| bytes == b"\x1b[32m"));
        let mut projection = effigy_core::terminal_projection::PlainStream::default();
        let plain = projection.project(&lines[0]);
        assert_eq!(plain, b"Compiling api\nerror");
    }

    #[test]
    fn underlay_shape_through_host_follower_keeps_colour_and_projects_incrementally() {
        use std::io::Write as _;
        let dir = tempfile::tempdir().expect("fixture dir");
        let path = dir
            .path()
            .join(".effigy/runtime/host-processes/dev/api.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"\x1b[32mCompiling\x1b[0m api\n").unwrap();
        let mut tab = tab(HostProcessRestart::Never, None);
        tab.log = path.clone();
        let mut follower = HostFollower::new(tab);
        let mut projection = effigy_core::terminal_projection::PlainStream::default();
        let mut raw = Vec::new();
        let mut plain = Vec::new();
        for line in follower.poll().lines {
            raw.extend_from_slice(&line);
            raw.push(b'\n');
            plain.extend(projection.project(&line));
            plain.extend(projection.project(b"\n"));
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(b"\x1b]title\nsecret payload\x1b\\\x1b[31merror\x1b[0m\n")
            .unwrap();
        for line in follower.poll().lines {
            raw.extend_from_slice(&line);
            raw.push(b'\n');
            plain.extend(projection.project(&line));
            plain.extend(projection.project(b"\n"));
        }
        assert!(raw.windows(5).any(|part| part == b"\x1b[32m"));
        assert_eq!(plain, b"Compiling api\nerror\n");
        assert_eq!(
            plain,
            effigy_core::terminal_projection::PlainStream::default().project(&raw)
        );
    }

    #[test]
    fn replacing_log_with_larger_file_resets_generation() {
        let dir = tempfile::tempdir().expect("log dir");
        let log = dir.path().join("api.log");
        let next = dir.path().join("next.log");
        std::fs::write(&log, "old\n").unwrap();
        let mut follower = LogTail::new();
        assert_eq!(follower.read(&log), vec![b"old".to_vec()]);
        std::fs::write(&next, "new longer generation\n").unwrap();
        std::fs::rename(&next, &log).unwrap();
        assert_eq!(follower.read(&log), vec![b"new longer generation".to_vec()]);
        assert!(follower.generation_changed);
    }

    #[test]
    fn plain_host_reader_retains_raw_c1_until_projection() {
        let dir = tempfile::tempdir().expect("log dir");
        let log = dir.path().join("api.log");
        std::fs::write(&log, b"visible \x9dhidden\x07done\n").unwrap();
        let mut follower = LogTail::new();
        let lines = follower.read(&log);
        let mut projection = effigy_core::terminal_projection::PlainStream::default();
        assert_eq!(projection.project(&lines[0]), b"visible done");
    }
}
