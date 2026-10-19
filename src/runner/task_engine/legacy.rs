//! v0.14 task state (contract 052 §11). Lease locks, status v1 records and
//! the task cache are never imported, modified or deleted (M3). A live
//! legacy lock holder on this host still refuses its task until it exits;
//! anything else is ignored and listed by `doctor` until removed by hand.

use std::fs;
use std::path::{Path, PathBuf};

use effigy_kernel::ident;
use effigy_kernel::observe::Observed;
use serde::{Deserialize, Serialize};

use super::identity::TaskTarget;
use crate::runner::error::RunnerError;

const LOCKS_DIR: &str = ".effigy/locks";
const STATUS_ACTIVE_DIR: &str = ".effigy/runtime/tasks";
const STATUS_REPORTS_DIR: &str = ".effigy/reports/tasks";
const CACHE_DIR: &str = ".effigy/cache";
/// Bounded listing: legacy directories can be large.
const MAX_ENTRIES: usize = 4096;

#[derive(Debug, Deserialize)]
struct LegacyLockRecord {
    scope: String,
    pid: u32,
    #[serde(default)]
    hostname: Option<String>,
}

/// Refuses a run while a v0.14 holder of the same lock is still alive on
/// this host. Foreign, unreadable or dead records are ignored, never
/// deleted (M3, M4).
pub(in crate::runner) fn refuse_live_legacy_holder(target: &TaskTarget) -> Result<(), RunnerError> {
    let Ok(entries) = fs::read_dir(target.root.join(LOCKS_DIR)) else {
        return Ok(());
    };
    let host = local_hostname();
    for entry in entries.flatten().take(MAX_ENTRIES) {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("lock") {
            continue;
        }
        let Some(record) = fs::read(&path)
            .ok()
            .and_then(|body| serde_json::from_slice::<LegacyLockRecord>(&body).ok())
        else {
            continue;
        };
        if !names_target(&record.scope, target) {
            continue;
        }
        let same_host = record
            .hostname
            .as_deref()
            .is_none_or(|recorded| host.as_deref() == Some(recorded));
        if !same_host || record.pid == std::process::id() {
            continue;
        }
        if matches!(ident::process_identity(record.pid), Observed::Verified(..)) {
            return Err(RunnerError::task_invocation(format!(
                "`{}` is held by a live run of an earlier Effigy (pid {}, lock {}). It keeps \
                 running under its own lock; this run waits for nothing and starts nothing. \
                 Retry once pid {} exits",
                target.label,
                record.pid,
                path.display(),
                record.pid
            )));
        }
    }
    Ok(())
}

fn names_target(scope: &str, target: &TaskTarget) -> bool {
    if let (Some(name), Some(shared)) = (scope.strip_prefix("shared:"), &target.shared_lock) {
        return name == shared;
    }
    let Some(selector) = scope.strip_prefix("task:") else {
        return false;
    };
    selector == target.label
        || selector == target.task
        || selector
            .rsplit_once('/')
            .is_some_and(|(_, task)| task == target.task)
}

fn local_hostname() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: writable buffer of the given length; gethostname NUL-terminates
    // on success within it.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return None;
    }
    let end = buffer.iter().position(|byte| *byte == 0)?;
    String::from_utf8(buffer[..end].to_vec()).ok()
}

/// One legacy state family present under a root.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct LegacyFamily {
    pub(in crate::runner) family: &'static str,
    pub(in crate::runner) path: PathBuf,
    pub(in crate::runner) entries: usize,
    pub(in crate::runner) disposition: &'static str,
}

/// Read-only inventory of v0.14 task state under `root`, for `doctor`.
pub(in crate::runner) fn legacy_families(root: &Path) -> Vec<LegacyFamily> {
    [
        (
            "task-lease-locks",
            LOCKS_DIR,
            "ignored: run gates replace lease locks; a live holder still refuses its task until it exits",
        ),
        (
            "task-status-v1-active",
            STATUS_ACTIVE_DIR,
            "ignored: status reads run records; shown read-only for tasks with no runs yet",
        ),
        (
            "task-status-v1-reports",
            STATUS_REPORTS_DIR,
            "ignored: status reads run records; shown read-only for tasks with no runs yet",
        ),
        (
            "task-cache",
            CACHE_DIR,
            "ignored: the task cache was removed; delete the directory",
        ),
    ]
    .into_iter()
    .filter_map(|(family, relative, disposition)| {
        let path = root.join(relative);
        let entries = fs::read_dir(&path).ok()?.take(MAX_ENTRIES).count();
        (entries > 0).then_some(LegacyFamily {
            family,
            path,
            entries,
            disposition,
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_manifest::LoadedCatalog;

    fn target(root: &Path) -> TaskTarget {
        let catalog = LoadedCatalog {
            alias: String::new(),
            catalog_root: root.to_path_buf(),
            manifest_path: root.join("effigy.toml"),
            bundle_root: None,
            manifest: toml::from_str("").expect("empty manifest"),
            defer_run: None,
            deferred_builtins: Default::default(),
            depth: 0,
            draft_sources: Default::default(),
        };
        TaskTarget::for_catalog(root, "check", &catalog)
    }

    fn write_lock(root: &Path, name: &str, pid: u32) {
        let locks = root.join(LOCKS_DIR);
        fs::create_dir_all(&locks).expect("locks");
        fs::write(
            locks.join(name),
            format!(r#"{{"scope":"task:check","pid":{pid},"started_at_epoch_ms":1}}"#),
        )
        .expect("lock");
    }

    #[test]
    fn live_legacy_holder_refuses_and_dead_or_foreign_ones_are_ignored() {
        let root = tempfile::tempdir().expect("root");
        write_lock(root.path(), "task-check.lock", 999_999_999);
        refuse_live_legacy_holder(&target(root.path())).expect("dead holder is ignored");

        let mut live = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("live fixture");
        write_lock(root.path(), "task-check.lock", live.id());
        let error = refuse_live_legacy_holder(&target(root.path())).expect_err("live holder");
        assert!(error.to_string().contains("earlier Effigy"), "{error}");
        assert!(root.path().join(LOCKS_DIR).join("task-check.lock").exists());
        let _ = live.kill();
        let _ = live.wait();
    }

    #[test]
    fn doctor_lists_legacy_families_without_touching_them() {
        let root = tempfile::tempdir().expect("root");
        write_lock(root.path(), "task-check.lock", 1);
        fs::create_dir_all(root.path().join(CACHE_DIR)).expect("cache");
        fs::write(root.path().join(CACHE_DIR).join("task-cache-v1.json"), "{}").expect("cache");
        let families = legacy_families(root.path());
        let names = families
            .iter()
            .map(|family| family.family)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["task-lease-locks", "task-cache"]);
        assert!(root
            .path()
            .join(CACHE_DIR)
            .join("task-cache-v1.json")
            .exists());
    }
}
