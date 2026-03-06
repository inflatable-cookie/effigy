use std::path::Path;
use std::time::UNIX_EPOCH;

use super::fingerprint::compute_fingerprint_snapshot;
use super::model::{TaskCacheCheck, TaskCacheEntry};
use super::store::{load_cache_store, save_cache_store};
use crate::runner::error::RunnerError;
use crate::runner::manifest::task_runtime::{ManifestTask, ManifestTaskCache};

pub(in crate::runner) fn cache_entry_key(manifest_path: &Path, task_name: &str) -> String {
    format!("{}::{task_name}", manifest_path.display())
}

pub(in crate::runner) fn task_cache_config(task: &ManifestTask) -> Option<&ManifestTaskCache> {
    task.cache.as_ref().filter(|config| config.enabled)
}

pub(in crate::runner) fn check_task_cache(
    workspace_root: &Path,
    catalog_root: &Path,
    manifest_path: &Path,
    task_name: &str,
    task: &ManifestTask,
    command: &str,
) -> Result<TaskCacheCheck, RunnerError> {
    let Some(config) = task_cache_config(task) else {
        return Ok(TaskCacheCheck {
            enabled: false,
            hit: false,
            reason: "cache disabled".to_owned(),
            fingerprint: String::new(),
        });
    };

    let snapshot = compute_fingerprint_snapshot(command, config, catalog_root)?;
    let key = cache_entry_key(manifest_path, task_name);
    let store = load_cache_store(workspace_root)?;
    let hit = store
        .entries
        .get(&key)
        .map(|entry| entry.fingerprint == snapshot.fingerprint)
        .unwrap_or(false)
        && snapshot.outputs_exist;

    let reason = if hit {
        "fingerprint matched and declared outputs exist".to_owned()
    } else if !snapshot.outputs_exist {
        "declared outputs are missing".to_owned()
    } else {
        "fingerprint changed or no prior cache entry".to_owned()
    };

    Ok(TaskCacheCheck {
        enabled: true,
        hit,
        reason,
        fingerprint: snapshot.fingerprint,
    })
}

pub(in crate::runner) fn update_task_cache_entry(
    workspace_root: &Path,
    catalog_root: &Path,
    manifest_path: &Path,
    task_name: &str,
    task: &ManifestTask,
    command: &str,
) -> Result<(), RunnerError> {
    let Some(config) = task_cache_config(task) else {
        return Ok(());
    };

    let snapshot = compute_fingerprint_snapshot(command, config, catalog_root)?;
    let mut store = load_cache_store(workspace_root)?;
    let key = cache_entry_key(manifest_path, task_name);
    let entry = TaskCacheEntry {
        key: key.clone(),
        task_name: task_name.to_owned(),
        manifest_path: manifest_path.display().to_string(),
        catalog_root: catalog_root.display().to_string(),
        fingerprint: snapshot.fingerprint,
        command: command.to_owned(),
        inputs: config.inputs.clone(),
        outputs: config.outputs.clone(),
        env_keys: config.env.clone(),
        outputs_exist: snapshot.outputs_exist,
        updated_at_epoch_ms: now_epoch_ms(),
    };
    store.entries.insert(key, entry);
    save_cache_store(workspace_root, &store)
}

pub(in crate::runner) fn cache_entries(
    workspace_root: &Path,
) -> Result<Vec<TaskCacheEntry>, RunnerError> {
    let store = load_cache_store(workspace_root)?;
    let mut entries = store.entries.into_values().collect::<Vec<TaskCacheEntry>>();
    entries.sort_by(|a, b| {
        a.task_name
            .cmp(&b.task_name)
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    Ok(entries)
}

pub(in crate::runner) fn cache_entry(
    workspace_root: &Path,
    manifest_path: &Path,
    task_name: &str,
) -> Result<Option<TaskCacheEntry>, RunnerError> {
    let key = cache_entry_key(manifest_path, task_name);
    let store = load_cache_store(workspace_root)?;
    Ok(store.entries.get(&key).cloned())
}

pub(in crate::runner) fn invalidate_cache_keys(
    workspace_root: &Path,
    keys: &[String],
) -> Result<Vec<String>, RunnerError> {
    let mut store = load_cache_store(workspace_root)?;
    let mut removed = Vec::new();
    for key in keys {
        if store.entries.remove(key).is_some() {
            removed.push(key.clone());
        }
    }
    save_cache_store(workspace_root, &store)?;
    Ok(removed)
}

pub(in crate::runner) fn invalidate_all_cache_entries(
    workspace_root: &Path,
) -> Result<usize, RunnerError> {
    let mut store = load_cache_store(workspace_root)?;
    let removed = store.entries.len();
    store.entries.clear();
    save_cache_store(workspace_root, &store)?;
    Ok(removed)
}

fn now_epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
