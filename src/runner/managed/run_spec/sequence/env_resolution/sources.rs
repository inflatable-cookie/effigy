use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::ManifestEnvEntry;
use crate::runner::{LoadedCatalog, RunnerError};

use super::super::pathing::{
    find_catalog_by_normalized_root, resolve_catalog_reference_root, split_catalog_env_reference,
};

pub(super) fn resolve_manifest_env_entry<'a>(
    entry_ref: &str,
    local_env_entries: &'a BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &'a [LoadedCatalog],
) -> Option<(String, &'a ManifestEnvEntry)> {
    if let Some(local) = local_env_entries.get(entry_ref) {
        return Some((entry_ref.to_owned(), local));
    }
    let (catalog_path, env_key) = split_catalog_env_reference(entry_ref)?;

    let target_catalog_root = resolve_catalog_reference_root(catalog_path, repo_root);
    let target_catalog = find_catalog_by_normalized_root(catalogs, &target_catalog_root)?;
    let entry = target_catalog.manifest.env.get(env_key)?;
    Some((env_key.to_owned(), entry))
}

pub(super) fn resolve_process_env_entry(entry_ref: &str) -> Option<(String, String)> {
    if split_catalog_env_reference(entry_ref).is_some() {
        return None;
    }
    std::env::var(entry_ref)
        .ok()
        .map(|value| (entry_ref.to_owned(), value))
}

pub(super) fn unknown_env_entry_error(task_name: &str, entry_ref: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "task `{task_name}` run step references unknown env entry `{entry_ref}`"
    ))
}
