//! Cheap, non-blocking graph health snapshot.
//!
//! Read entirely through the filesystem. A graph command that blew its time
//! budget has to be able to say *why* without opening the SQLite store the
//! stalled work may still be holding, so nothing here touches the database.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::json::GraphLockPayload;
use crate::paths::GraphPaths;
use crate::refresh::inspect_refresh_lock;

/// Index and refresh-worker state behind a graph command.
///
/// Emitted in the JSON error envelope when a graph command exceeds its time
/// budget, so an agent can tell "another process is mid-refresh, retry" from
/// "no graph index exists and building one is slow".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphHealthPayload {
    pub repo_root: String,
    pub db_path: String,
    /// Whether a graph database file exists at all.
    pub index_present: bool,
    pub db_size_bytes: u64,
    pub refresh_lock_path: String,
    /// Whether some process currently holds the cross-process refresh lock.
    pub refresh_in_progress: bool,
    /// Holder pid, age, and liveness when the lock file records them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_lock: Option<GraphLockPayload>,
    /// One-line reading of the two flags above.
    pub summary: String,
}

/// Snapshot graph health for `repo_root` without blocking on graph work.
pub fn health(repo_root: &Path) -> GraphHealthPayload {
    let paths = GraphPaths::for_repo(repo_root);
    let db_metadata = std::fs::metadata(&paths.db_path).ok();
    let index_present = db_metadata.as_ref().is_some_and(std::fs::Metadata::is_file);
    let db_size_bytes = db_metadata.map(|metadata| metadata.len()).unwrap_or(0);
    let inspection = inspect_refresh_lock(&paths.refresh_lock_path);
    let refresh_in_progress = inspection.held;
    let refresh_lock =
        (inspection.held || inspection.identity.is_some()).then(|| inspection.payload());
    let summary = match (
        index_present,
        refresh_in_progress,
        inspection.identity.as_ref(),
    ) {
        (_, true, Some(identity)) if identity.stale_holder => {
            format!(
                "a graph refresh lock is held by stale pid {} (age {}ms); retry `effigy graph index --json` or pass `--stale-index`",
                identity.pid, identity.age_ms
            )
        }
        (_, true, Some(identity)) => format!(
            "a graph refresh is in progress (pid {}, age {}ms); retry once it releases the lock or pass `--stale-index`",
            identity.pid, identity.age_ms
        ),
        (_, true, None) => {
            "a graph refresh is in progress; retry once it releases the refresh lock".to_owned()
        }
        (true, false, _) => "a graph index exists and no refresh holds the lock".to_owned(),
        (false, false, _) => {
            "no graph index exists yet; the first build walks the whole repo".to_owned()
        }
    };
    GraphHealthPayload {
        repo_root: repo_root.display().to_string(),
        db_path: paths.db_path.display().to_string(),
        index_present,
        db_size_bytes,
        refresh_lock_path: paths.refresh_lock_path.display().to_string(),
        refresh_in_progress,
        refresh_lock,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::health;

    #[test]
    fn health_reports_a_missing_index_without_creating_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let payload = health(dir.path());
        assert!(!payload.index_present);
        assert!(!payload.refresh_in_progress);
        assert_eq!(payload.db_size_bytes, 0);
        assert!(payload.summary.contains("no graph index exists yet"));
        assert!(!dir.path().join(".effigy").exists());
    }
}
