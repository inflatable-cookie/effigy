use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub(in crate::runner) struct TaskCacheCheck {
    pub(in crate::runner) enabled: bool,
    pub(in crate::runner) hit: bool,
    pub(in crate::runner) reason: String,
    pub(in crate::runner) fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::runner) struct TaskCacheEntry {
    pub(in crate::runner) key: String,
    pub(in crate::runner) task_name: String,
    pub(in crate::runner) manifest_path: String,
    pub(in crate::runner) catalog_root: String,
    pub(in crate::runner) fingerprint: String,
    pub(in crate::runner) command: String,
    pub(in crate::runner) inputs: Vec<String>,
    pub(in crate::runner) outputs: Vec<String>,
    pub(in crate::runner) env_keys: Vec<String>,
    pub(in crate::runner) outputs_exist: bool,
    pub(in crate::runner) updated_at_epoch_ms: u128,
}
