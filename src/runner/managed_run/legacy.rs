//! v0.14 managed sessions (contract 052 §11). `session.json`, stop markers,
//! exec-group markers and lifecycle state files are never imported, modified
//! or deleted (M3). A session whose recorded supervisor PID is alive still
//! refuses a new run of its task and profile: a PID-only owner is `Unknown`
//! (M4), so nothing here signals it; the refusal names the earlier Effigy's
//! own stop control, kept for one minor.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::control::Addressed;
use crate::runner::error::RunnerError;

const SESSION_SCHEMA: &str = "effigy.managed.headless.v1";
const SESSIONS_DIR: &str = ".effigy/runtime/managed";

#[derive(Debug, Deserialize)]
struct LegacySession {
    schema: String,
    supervisor_pid: u32,
    status: String,
}

/// The v0.14 session directory of one task and profile.
fn session_path(root: &Path, task: &str, profile: &str) -> PathBuf {
    root.join(SESSIONS_DIR)
        .join(format!("{}-{}", component(task), component(profile)))
        .join("session.json")
}

fn component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

/// A live v0.14 session of this task and profile, described for the user.
pub(in crate::runner) fn live_session(addressed: &Addressed<'_>) -> Option<String> {
    let path = session_path(&addressed.target.root, addressed.task, addressed.profile);
    let raw = std::fs::read(&path).ok()?;
    let session: LegacySession = serde_json::from_slice(&raw).ok()?;
    if session.schema != SESSION_SCHEMA || session.status != "running" {
        return None;
    }
    let alive = matches!(
        effigy_kernel::ident::process_identity(session.supervisor_pid),
        effigy_kernel::observe::Observed::Verified(..)
    );
    alive.then(|| {
        format!(
            "an earlier Effigy's managed session ({}) records a live supervisor, pid {}. It is \
             not signalled by this Effigy: stop it with the Effigy that started it \
             (`effigy {} stop`), or end pid {} yourself once you have checked it",
            path.display(),
            session.supervisor_pid,
            addressed.task,
            session.supervisor_pid
        )
    })
}

/// Refuses a new run beside a live v0.14 session of the same task and profile.
pub(in crate::runner) fn refuse_live_session(addressed: &Addressed<'_>) -> Result<(), RunnerError> {
    match live_session(addressed) {
        Some(reason) => Err(RunnerError::task_invocation(format!(
            "`{}` (profile `{}`) cannot start: {reason}",
            addressed.task, addressed.profile
        ))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::task_engine::TaskTarget;

    fn target(root: &Path) -> TaskTarget {
        TaskTarget {
            root: root.to_path_buf(),
            catalog: String::new(),
            task: "dev".to_owned(),
            surface: effigy_core::task_selection::TaskSurface::Published,
            profile: Some("default".to_owned()),
            shared_lock: None,
            label: "dev".to_owned(),
        }
    }

    fn write(root: &Path, pid: u32, status: &str) {
        let path = session_path(root, "dev", "default");
        std::fs::create_dir_all(path.parent().expect("session dir")).expect("session dir");
        std::fs::write(
            &path,
            format!(
                r#"{{"schema":"{SESSION_SCHEMA}","task":"dev","profile":"default","supervisor_pid":{pid},"status":"{status}","started_at_unix_ms":1,"processes":[]}}"#
            ),
        )
        .expect("session");
    }

    #[test]
    fn a_live_legacy_session_refuses_and_is_never_modified() {
        let root = tempfile::tempdir().expect("root");
        let target = target(root.path());
        let addressed = Addressed {
            target: &target,
            task: "dev",
            profile: "default",
        };
        write(root.path(), std::process::id(), "running");
        let before = std::fs::read(session_path(root.path(), "dev", "default")).expect("read");
        let error = refuse_live_session(&addressed).expect_err("live legacy session");
        assert!(error.to_string().contains("earlier Effigy"), "{error}");
        assert_eq!(
            std::fs::read(session_path(root.path(), "dev", "default")).expect("read"),
            before
        );
        write(root.path(), 999_999_999, "running");
        refuse_live_session(&addressed).expect("dead legacy session is ignored");
        write(root.path(), std::process::id(), "stopped");
        refuse_live_session(&addressed).expect("stopped legacy session is ignored");
    }
}
