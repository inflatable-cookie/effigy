use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;
use crate::runner::manifest::ManifestEnvEntry;
use crate::runner::model::catalog::LoadedCatalog;

use super::super::pathing::{
    find_catalog_by_normalized_root, normalize_path, resolve_catalog_reference_root,
    split_catalog_env_reference,
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

pub(super) fn resolve_env_schema_entry(
    entry_ref: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&Path>,
    env_schema_cache: &mut BTreeMap<PathBuf, Option<BTreeMap<String, String>>>,
) -> Result<Option<(String, String)>, RunnerError> {
    let (target_root, env_key) = match split_catalog_env_reference(entry_ref) {
        Some((catalog_path, env_key)) => (
            resolve_catalog_reference_root(catalog_path, repo_root),
            env_key,
        ),
        None => (normalize_path(repo_root), entry_ref),
    };
    let normalized_root = normalize_path(&target_root);

    if !env_schema_cache.contains_key(&normalized_root) {
        let Some(target_catalog) = find_catalog_by_normalized_root(catalogs, &normalized_root)
        else {
            env_schema_cache.insert(normalized_root.clone(), None);
            return Ok(None);
        };
        let resolved = crate::runner::env_schema_support::resolve_catalog_env_schema(
            &target_catalog.catalog_root,
            target_catalog.manifest.env_schema.as_ref(),
            runtime_env_schema_override,
        )?;
        env_schema_cache.insert(normalized_root.clone(), resolved.map(|env| env.plain_env()));
    }

    Ok(env_schema_cache
        .get(&normalized_root)
        .and_then(|cached| cached.as_ref())
        .and_then(|entries| entries.get(env_key))
        .cloned()
        .map(|value| (env_key.to_owned(), value)))
}

pub(super) fn unknown_env_entry_error(task_name: &str, entry_ref: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "task `{task_name}` run step references unknown env entry `{entry_ref}`"
    ))
}
