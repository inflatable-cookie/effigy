//! Cheap, non-blocking graph health snapshot.
//!
//! Read entirely through the filesystem. A graph command that blew its time
//! budget has to be able to say *why* without opening the SQLite store the
//! stalled work may still be holding, so nothing here touches the database.

use std::fs::OpenOptions;
use std::path::Path;

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::paths::GraphPaths;

/// Index and refresh-worker state behind a graph command.
///
/// Emitted in the JSON error envelope when a graph command exceeds its time
/// budget, so an agent can tell "another process is mid-refresh, retry" from
/// "no index exists and building one is slow".
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
    /// One-line reading of the two flags above.
    pub summary: String,
}

/// Snapshot graph health for `repo_root` without blocking on graph work.
pub fn health(repo_root: &Path) -> GraphHealthPayload {
    let paths = GraphPaths::for_repo(repo_root);
    let db_metadata = std::fs::metadata(&paths.db_path).ok();
    let index_present = db_metadata.as_ref().is_some_and(std::fs::Metadata::is_file);
    let db_size_bytes = db_metadata.map(|metadata| metadata.len()).unwrap_or(0);
    let refresh_in_progress = refresh_lock_held(&paths.refresh_lock_path);
    let summary = match (index_present, refresh_in_progress) {
        (_, true) => "a graph refresh is in progress; retry once it releases the refresh lock",
        (true, false) => "a graph index exists and no refresh holds the lock",
        (false, false) => "no graph index exists yet; the first build walks the whole repo",
    }
    .to_owned();
    GraphHealthPayload {
        repo_root: repo_root.display().to_string(),
        db_path: paths.db_path.display().to_string(),
        index_present,
        db_size_bytes,
        refresh_lock_path: paths.refresh_lock_path.display().to_string(),
        refresh_in_progress,
        summary,
    }
}

/// Whether the refresh lock is currently held. Never creates the lock file:
/// an absent lock file means no refresh has ever run here.
fn refresh_lock_held(lock_path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().read(true).write(true).open(lock_path) else {
        return false;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            false
        }
        Err(error) => error.kind() == std::io::ErrorKind::WouldBlock,
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
