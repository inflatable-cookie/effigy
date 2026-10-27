//! v0.14 task state (contract 052 §11). Lease locks, status v1 records and
//! retired cache files are never imported, modified or deleted (M3). Nothing
//! reads them and nothing refuses because of them; `doctor` lists what is left
//! so an operator can delete it by hand.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

const STATUS_ACTIVE_DIR: &str = ".effigy/runtime/tasks";
const STATUS_REPORTS_DIR: &str = ".effigy/reports/tasks";
const LOCKS_DIR: &str = ".effigy/locks";
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
            "ignored: run gates replace lease locks; delete the directory",
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
