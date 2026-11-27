//! Relay records under `.effigy/runtime/fs-events` and their status projection.
//!
//! One environment has at most one record, `<container>.json`, written by the
//! process that holds the relay: an attached session or the stack-owned relay.
//! The record's stream states live in `<container>.streams/`.

use super::detect::{Backend, FsEventsState};
use crate::{EffectiveContainerPolicy, EffectiveFsEventsMode};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

pub const FS_RELAY_RECORD_SCHEMA: &str = "effigy.fs-events.relay.v1";
const FS_RELAY_DIR: &str = ".effigy/runtime/fs-events";

/// Which lifetime owns a running relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayOwner {
    /// An attached command; the relay ends with it.
    Session,
    /// The detached environment; the relay ends with the stack.
    Stack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayRecordTarget {
    pub service: String,
    pub container: String,
    pub host: PathBuf,
    pub guest: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayRecord {
    pub schema: String,
    pub container: String,
    pub owner: RelayOwner,
    pub pid: u32,
    pub state: String,
    pub reason: String,
    pub targets: Vec<RelayRecordTarget>,
}

/// The `fs_events` field of `container status` and the doctor evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FsEventsStatus {
    /// The configured mode: `auto`, `on` or `off`.
    pub mode: &'static str,
    /// `active`, `reconnecting`, `inactive`, `noop`, `interrupted` or `unknown`.
    pub state: String,
    pub reason: String,
    pub owner: Option<RelayOwner>,
    pub pid: Option<u32>,
    pub targets: Vec<RelayRecordTarget>,
}

impl FsEventsStatus {
    /// Say why nothing runs on a backend whose containers already see host
    /// file events. A recorded relay is reported as it is.
    pub fn with_backend(mut self, backend: Backend) -> Self {
        if self.reason != "not_running" {
            return self;
        }
        let reason = match backend {
            Backend::LinuxDockerEngine => "native_kernel_inotify",
            Backend::OrbStack => "orbstack_native_inotify",
            Backend::Other => return self,
        };
        self.state = state_label(FsEventsState::Noop).to_owned();
        self.reason = reason.to_owned();
        self
    }

    /// `active (explicit_on, targets=app:/workspace)`.
    pub fn summary(&self) -> String {
        let targets = if self.targets.is_empty() {
            "none".to_owned()
        } else {
            self.targets
                .iter()
                .map(|target| format!("{}:{}", target.service, target.guest.display()))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!("{} ({}, targets={targets})", self.state, self.reason)
    }
}

pub fn state_label(state: FsEventsState) -> &'static str {
    match state {
        FsEventsState::Active => "active",
        FsEventsState::Inactive => "inactive",
        FsEventsState::Noop => "noop",
    }
}

pub fn mode_label(mode: EffectiveFsEventsMode) -> &'static str {
    match mode {
        EffectiveFsEventsMode::Auto => "auto",
        EffectiveFsEventsMode::On => "on",
        EffectiveFsEventsMode::Off => "off",
    }
}

fn file_stem(container: &str) -> String {
    container
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn fs_relay_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(FS_RELAY_DIR)
}

pub fn fs_relay_record_path(repo_root: &Path, container: &str) -> PathBuf {
    fs_relay_dir(repo_root).join(format!("{}.json", file_stem(container)))
}

pub(super) fn lock_path(repo_root: &Path, container: &str) -> PathBuf {
    fs_relay_dir(repo_root).join(format!("{}.lock", file_stem(container)))
}

pub(super) fn stream_state_dir(repo_root: &Path, container: &str) -> PathBuf {
    fs_relay_dir(repo_root).join(format!("{}.streams", file_stem(container)))
}

pub fn read_fs_relay_record(repo_root: &Path, container: &str) -> io::Result<Option<RelayRecord>> {
    let bytes = match fs::read(fs_relay_record_path(repo_root, container)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let record: RelayRecord = serde_json::from_slice(&bytes)?;
    if record.schema != FS_RELAY_RECORD_SCHEMA {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "unsupported relay record schema",
        ));
    }
    Ok(Some(record))
}

pub(super) fn write_record(repo_root: &Path, record: &RelayRecord) -> io::Result<()> {
    let path = fs_relay_record_path(repo_root, &record.container);
    let temp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(&temp, serde_json::to_vec_pretty(record)?)?;
    fs::rename(temp, path)
}

fn process_alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // `EPERM` still names a live process.
    pid > 0
        && !matches!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH)
        )
}

/// Remove the record a relay left behind when it died without cleaning up.
/// A live owner's record is never touched.
pub fn clear_dead_fs_relay_record(repo_root: &Path, container: &str) {
    if let Ok(Some(record)) = read_fs_relay_record(repo_root, container) {
        if !process_alive(record.pid) {
            let _ = fs::remove_file(fs_relay_record_path(repo_root, container));
            let _ = fs::remove_dir_all(stream_state_dir(repo_root, container));
        }
    }
}

/// Whether a stack-owned relay has published its record for `container` and
/// its process is alive. Whatever state the relay reports, it is up.
pub fn stack_relay_published(repo_root: &Path, container: &str) -> bool {
    matches!(
        read_fs_relay_record(repo_root, container),
        Ok(Some(record)) if record.owner == RelayOwner::Stack && process_alive(record.pid)
    )
}

fn any_stream_reconnecting(repo_root: &Path, container: &str) -> bool {
    let Ok(entries) = fs::read_dir(stream_state_dir(repo_root, container)) else {
        return false;
    };
    entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .any(|value| value["state"] == "reconnecting")
}

/// Read-only projection of one environment's relay: the recorded relay when
/// there is one, otherwise what the configuration alone decides.
pub fn fs_events_status(policy: &EffectiveContainerPolicy) -> FsEventsStatus {
    let mode = mode_label(policy.fs_events.mode);
    let idle = |state: &str, reason: &str| FsEventsStatus {
        mode,
        state: state.to_owned(),
        reason: reason.to_owned(),
        owner: None,
        pid: None,
        targets: Vec::new(),
    };
    match read_fs_relay_record(&policy.repo_root, &policy.name) {
        Ok(Some(record)) => {
            let (state, reason) = if !process_alive(record.pid) {
                ("interrupted".to_owned(), "owner_exited".to_owned())
            } else if record.state == "active"
                && any_stream_reconnecting(&policy.repo_root, &policy.name)
            {
                ("reconnecting".to_owned(), record.reason)
            } else {
                (record.state, record.reason)
            };
            FsEventsStatus {
                mode,
                state,
                reason,
                owner: Some(record.owner),
                pid: Some(record.pid),
                targets: record.targets,
            }
        }
        Ok(None) if policy.fs_events.mode == EffectiveFsEventsMode::Off => {
            idle("inactive", "explicit_off")
        }
        Ok(None) => idle("inactive", "not_running"),
        Err(_) => idle("unknown", "record_unreadable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_projects_config_live_record_and_dead_owner() {
        let dir = tempfile::tempdir().expect("fixture");
        let mut policy = crate::policy::model::test_policy(dir.path());
        policy.name = "web".into();
        assert_eq!(fs_events_status(&policy).reason, "not_running");
        assert_eq!(
            fs_events_status(&policy)
                .with_backend(Backend::OrbStack)
                .state,
            "noop"
        );
        policy.fs_events.mode = EffectiveFsEventsMode::Off;
        assert_eq!(fs_events_status(&policy).reason, "explicit_off");
        policy.fs_events.mode = EffectiveFsEventsMode::On;

        fs::create_dir_all(stream_state_dir(dir.path(), "web")).expect("streams");
        let mut record = RelayRecord {
            schema: FS_RELAY_RECORD_SCHEMA.into(),
            container: "web".into(),
            owner: RelayOwner::Stack,
            pid: std::process::id(),
            state: "active".into(),
            reason: "explicit_on".into(),
            targets: vec![RelayRecordTarget {
                service: "app".into(),
                container: "abc".into(),
                host: dir.path().into(),
                guest: "/workspace".into(),
            }],
        };
        write_record(dir.path(), &record).expect("record");
        assert_eq!(
            fs_events_status(&policy).summary(),
            "active (explicit_on, targets=app:/workspace)"
        );
        clear_dead_fs_relay_record(dir.path(), "web");
        assert!(fs_relay_record_path(dir.path(), "web").exists());
        fs::write(
            stream_state_dir(dir.path(), "web").join("abc.json"),
            r#"{"state":"reconnecting"}"#,
        )
        .expect("stream state");
        assert_eq!(fs_events_status(&policy).state, "reconnecting");

        // A reaped child's PID is a dead owner.
        let mut child = std::process::Command::new("true").spawn().expect("child");
        record.pid = child.id();
        child.wait().expect("reap");
        write_record(dir.path(), &record).expect("record");
        let status = fs_events_status(&policy);
        assert_eq!(
            (status.state.as_str(), status.reason.as_str()),
            ("interrupted", "owner_exited")
        );
        clear_dead_fs_relay_record(dir.path(), "web");
        assert!(!fs_relay_record_path(dir.path(), "web").exists());
    }
}
