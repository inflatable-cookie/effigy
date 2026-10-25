//! v0.14 task state (contract 052 §11). Lease locks, status v1 records and
//! retired cache files are never imported, modified or deleted (M3). A live
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
/// Retired v0.14 cache files, as family name and file name. The rest of
/// `.effigy/cache` is live: `cache/sources` holds the pinned bundle sources and
/// `cache/bundles` holds unimported bundles, so only these files are ever
/// offered for deletion.
const RETIRED_CACHE_FILES: [(&str, &str); 3] = [
    ("task-cache", "task-cache-v1.json"),
    ("cache-catalog-discovery-v1", "catalog-discovery-v1.json"),
    ("cache-implicit-deferral-v1", "implicit-deferral-v1.json"),
];
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
    let directories = [
        (
            "task-lease-locks",
            LOCKS_DIR,
            "ignored: run gates replace lease locks; a live holder still refuses its task until it exits",
        ),
        (
            "task-status-v1-active",
            STATUS_ACTIVE_DIR,
            "ignored: nothing reads v0.14 status records; delete the directory",
        ),
        (
            "task-status-v1-reports",
            STATUS_REPORTS_DIR,
            "ignored: nothing reads v0.14 status records; delete the directory",
        ),
    ];
    let mut families = Vec::new();
    for (family, relative, disposition) in directories {
        let path = root.join(relative);
        if let Some(entries) = directory_entries(&path) {
            families.push(LegacyFamily {
                family,
                path,
                entries,
                disposition,
            });
        }
    }
    for (family, file) in RETIRED_CACHE_FILES {
        let path = root.join(CACHE_DIR).join(file);
        if path.is_file() {
            families.push(LegacyFamily {
                family,
                path,
                entries: 1,
                disposition: "ignored: a retired v0.14 cache file; delete it",
            });
        }
    }
    families
}

fn directory_entries(path: &Path) -> Option<usize> {
    let entries = fs::read_dir(path).ok()?.take(MAX_ENTRIES).count();
    (entries > 0).then_some(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_manifest::LoadedCatalog;

    fn target(root: &Path) -> TaskTarget {
        let catalog = LoadedCatalog::for_test(
            String::new(),
            root,
            toml::from_str("").expect("empty manifest"),
            0,
        );
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
        for file in ["task-cache-v1.json", "catalog-discovery-v1.json"] {
            fs::write(root.path().join(CACHE_DIR).join(file), "{}").expect("cache");
        }
        let families = legacy_families(root.path());
        let names = families
            .iter()
            .map(|family| family.family)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "task-lease-locks",
                "task-cache",
                "cache-catalog-discovery-v1"
            ]
        );
        for family in families.iter().filter(|family| family.path.is_file()) {
            assert!(
                !family.disposition.contains("delete the directory"),
                "{} advises deleting its directory",
                family.family
            );
        }
        assert!(root
            .path()
            .join(CACHE_DIR)
            .join("task-cache-v1.json")
            .exists());
    }

    /// `.effigy/cache/sources` and `cache/bundles` are live, so a checkout that
    /// only has those must never be told to delete `.effigy/cache`.
    #[test]
    fn doctor_never_advises_deleting_live_cache_directories() {
        let root = tempfile::tempdir().expect("root");
        fs::create_dir_all(root.path().join(CACHE_DIR).join("sources/generation"))
            .expect("sources");
        fs::create_dir_all(root.path().join(CACHE_DIR).join("bundles/git")).expect("bundles");
        assert!(legacy_families(root.path()).is_empty());
    }
}
