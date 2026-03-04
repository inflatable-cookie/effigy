use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::TaskCacheEntry;
use crate::runner::RunnerError;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct TaskCacheStore {
    #[serde(default = "cache_store_schema")]
    schema: String,
    #[serde(default = "cache_store_schema_version")]
    schema_version: u8,
    #[serde(default)]
    pub(super) entries: BTreeMap<String, TaskCacheEntry>,
}

pub(super) fn load_cache_store(workspace_root: &Path) -> Result<TaskCacheStore, RunnerError> {
    let path = workspace_root
        .join(super::CACHE_DIR)
        .join(super::CACHE_STORE_FILE);
    if !path.exists() {
        return Ok(TaskCacheStore::default());
    }
    let raw = fs::read_to_string(&path).map_err(|error| RunnerError::TaskManifestRead {
        path: path.clone(),
        error,
    })?;
    serde_json::from_str::<TaskCacheStore>(&raw).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse task cache store {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn save_cache_store(
    workspace_root: &Path,
    store: &TaskCacheStore,
) -> Result<(), RunnerError> {
    let cache_root = workspace_root.join(super::CACHE_DIR);
    fs::create_dir_all(&cache_root).map_err(|error| RunnerError::TaskManifestRead {
        path: cache_root.clone(),
        error,
    })?;
    let path = cache_root.join(super::CACHE_STORE_FILE);
    let encoded = serde_json::to_string_pretty(store)
        .map_err(|error| RunnerError::Ui(format!("failed to encode task cache store: {error}")))?;
    fs::write(&path, encoded).map_err(|error| RunnerError::TaskManifestRead { path, error })
}

fn cache_store_schema() -> String {
    "effigy.task.cache.store.v1".to_owned()
}

fn cache_store_schema_version() -> u8 {
    1
}

impl Default for TaskCacheStore {
    fn default() -> Self {
        Self {
            schema: cache_store_schema(),
            schema_version: cache_store_schema_version(),
            entries: BTreeMap::new(),
        }
    }
}
