use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::super::super::scripts::command_names;
use crate::BuiltinError;
use effigy_routing::{load_effective_catalogs, RoutingError};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ManifestStamp {
    path: PathBuf,
    modified_epoch_ns: Option<u128>,
    len_bytes: Option<u64>,
    content_hash_fnv1a64: Option<u64>,
}

pub(super) fn discover_completion_candidates(
    repo_root: &Path,
) -> Result<(Vec<String>, Vec<ManifestStamp>), BuiltinError> {
    let mut candidates: BTreeSet<String> = command_names().into_iter().map(str::to_owned).collect();
    let mut manifest_stamps: Vec<ManifestStamp> = Vec::new();
    match load_effective_catalogs(repo_root) {
        Ok(catalogs) => {
            let visibility = effigy_tasks::TaskVisibilityIndex::new(&catalogs);
            for catalog in &catalogs {
                let loaded =
                    effigy_manifest::load_task_manifest_with_inspection(&catalog.manifest_path)
                        .map_err(effigy_routing::RoutingError::from)?;
                let sources = loaded
                    .evaluation_order
                    .into_iter()
                    .chain(loaded.value_sources.into_iter().map(|source| source.source));
                manifest_stamps.extend(sources.map(|source| read_manifest_stamp(&source)));
                for (task_name, task) in &catalog.manifest.tasks {
                    if visibility.effective(catalog, task_name, task).0
                        != effigy_manifest::ManifestTaskVisibility::Public
                    {
                        continue;
                    }
                    candidates.insert(task_name.clone());
                    candidates.insert(format!("{}/{}", catalog.alias, task_name));
                }
            }
        }
        Err(RoutingError::TaskCatalogsMissing { .. }) => {}
        Err(error) => return Err(error.into()),
    }
    manifest_stamps.sort_by(|a, b| a.path.cmp(&b.path));
    manifest_stamps.dedup_by(|a, b| a.path == b.path);

    Ok((
        candidates.into_iter().collect::<Vec<String>>(),
        manifest_stamps,
    ))
}

pub(super) fn manifest_stamps_unchanged(expected: &[ManifestStamp]) -> bool {
    expected
        .iter()
        .all(|stamp| read_manifest_stamp(&stamp.path) == *stamp)
}

fn read_manifest_stamp(path: &Path) -> ManifestStamp {
    let metadata = fs::metadata(path).ok();
    let modified_epoch_ns = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    let contents = fs::read(path).ok();
    let len_bytes = contents
        .as_ref()
        .map(|bytes| bytes.len() as u64)
        .or_else(|| metadata.as_ref().map(|value| value.len()));
    let content_hash_fnv1a64 = contents.as_ref().map(|bytes| fnv1a64(bytes));

    ManifestStamp {
        path: path.to_path_buf(),
        modified_epoch_ns,
        len_bytes,
        content_hash_fnv1a64,
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::discover_completion_candidates;

    #[test]
    fn completion_visibility_stamps_included_task_metadata() {
        let root = tempfile::tempdir().expect("fresh fixture");
        std::fs::write(
            root.path().join("effigy.toml"),
            "[manifest]\ninclude = ['tasks.toml']\n",
        )
        .expect("root");
        let fragment = root.path().join("tasks.toml");
        std::fs::write(
            &fragment,
            "[tasks.x]\nrun = 'echo x'\nvisibility = 'public'\n",
        )
        .expect("fragment");
        let (_, stamps) = discover_completion_candidates(root.path()).expect("initial inventory");
        assert!(super::manifest_stamps_unchanged(&stamps));
        std::fs::write(
            &fragment,
            "[tasks.x]\nrun = 'echo x'\nvisibility = 'internal'\n",
        )
        .expect("metadata change");
        assert!(!super::manifest_stamps_unchanged(&stamps));
        let (candidates, _) =
            discover_completion_candidates(root.path()).expect("updated inventory");
        assert!(!candidates.iter().any(|name| name == "x"));
    }

    #[test]
    fn completion_visibility_excludes_internal_and_referenced_tasks() {
        let root = tempfile::tempdir().expect("fresh fixture directory");
        std::fs::write(
            root.path().join("effigy.toml"),
            r#"
[catalog]
alias = "sample"
[tasks]
entry = [{ task = "step:helper" }, { task = "kept" }]
"step:helper" = "echo step:helper"
kept = { run = "echo kept", visibility = "public" }
hidden = { run = "echo hidden", visibility = "internal" }
"#,
        )
        .expect("fixture manifest");
        let (candidates, _) = discover_completion_candidates(root.path()).expect("completion");
        for public in ["entry", "sample/entry", "kept", "sample/kept"] {
            assert!(
                candidates.iter().any(|candidate| candidate == public),
                "{public}"
            );
        }
        for internal in [
            "step:helper",
            "sample/step:helper",
            "hidden",
            "sample/hidden",
        ] {
            assert!(
                !candidates.iter().any(|candidate| candidate == internal),
                "{internal}"
            );
        }
    }
}
