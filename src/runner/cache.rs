use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use globset::Glob;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::{ManifestTask, ManifestTaskCache, RunnerError};

const CACHE_DIR: &str = ".effigy/cache";
const CACHE_STORE_FILE: &str = "task-cache-v1.json";

#[derive(Debug)]
pub(super) struct TaskCacheCheck {
    pub(super) enabled: bool,
    pub(super) hit: bool,
    pub(super) reason: String,
    pub(super) fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TaskCacheEntry {
    pub(super) key: String,
    pub(super) task_name: String,
    pub(super) manifest_path: String,
    pub(super) catalog_root: String,
    pub(super) fingerprint: String,
    pub(super) command: String,
    pub(super) inputs: Vec<String>,
    pub(super) outputs: Vec<String>,
    pub(super) env_keys: Vec<String>,
    pub(super) outputs_exist: bool,
    pub(super) updated_at_epoch_ms: u128,
}

#[derive(Debug, Serialize, Deserialize)]
struct TaskCacheStore {
    #[serde(default = "cache_store_schema")]
    schema: String,
    #[serde(default = "cache_store_schema_version")]
    schema_version: u8,
    #[serde(default)]
    entries: BTreeMap<String, TaskCacheEntry>,
}

#[derive(Debug, Serialize)]
struct FingerprintMaterial {
    command: String,
    inputs: Vec<DeclaredInputStamp>,
    env: Vec<DeclaredEnvStamp>,
    outputs: Vec<DeclaredOutputStamp>,
}

#[derive(Debug, Serialize)]
struct DeclaredInputStamp {
    declaration: String,
    matches: Vec<PathStamp>,
}

#[derive(Debug, Serialize)]
struct DeclaredOutputStamp {
    declaration: String,
    exists: bool,
    matched: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeclaredEnvStamp {
    key: String,
    value: Option<String>,
}

#[derive(Debug, Serialize)]
struct PathStamp {
    path: String,
    kind: &'static str,
    exists: bool,
    size: Option<u64>,
    modified_epoch_ms: Option<u128>,
    digest: Option<String>,
}

#[derive(Debug)]
struct CacheFingerprintSnapshot {
    fingerprint: String,
    outputs_exist: bool,
}

pub(super) fn cache_entry_key(manifest_path: &Path, task_name: &str) -> String {
    format!("{}::{task_name}", manifest_path.display())
}

pub(super) fn task_cache_config(task: &ManifestTask) -> Option<&ManifestTaskCache> {
    task.cache.as_ref().filter(|config| config.enabled)
}

pub(super) fn check_task_cache(
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

pub(super) fn update_task_cache_entry(
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

pub(super) fn cache_entries(workspace_root: &Path) -> Result<Vec<TaskCacheEntry>, RunnerError> {
    let store = load_cache_store(workspace_root)?;
    let mut entries = store.entries.into_values().collect::<Vec<TaskCacheEntry>>();
    entries.sort_by(|a, b| {
        a.task_name
            .cmp(&b.task_name)
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    Ok(entries)
}

pub(super) fn cache_entry(
    workspace_root: &Path,
    manifest_path: &Path,
    task_name: &str,
) -> Result<Option<TaskCacheEntry>, RunnerError> {
    let key = cache_entry_key(manifest_path, task_name);
    let store = load_cache_store(workspace_root)?;
    Ok(store.entries.get(&key).cloned())
}

pub(super) fn invalidate_cache_keys(
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

pub(super) fn invalidate_all_cache_entries(workspace_root: &Path) -> Result<usize, RunnerError> {
    let mut store = load_cache_store(workspace_root)?;
    let removed = store.entries.len();
    store.entries.clear();
    save_cache_store(workspace_root, &store)?;
    Ok(removed)
}

fn compute_fingerprint_snapshot(
    command: &str,
    config: &ManifestTaskCache,
    catalog_root: &Path,
) -> Result<CacheFingerprintSnapshot, RunnerError> {
    let input_stamps = collect_input_stamps(&config.inputs, catalog_root)?;
    let env_stamps = collect_env_stamps(&config.env);
    let output_stamps = collect_output_stamps(&config.outputs, catalog_root)?;
    let outputs_exist = output_stamps.iter().all(|stamp| stamp.exists);
    let material = FingerprintMaterial {
        command: command.to_owned(),
        inputs: input_stamps,
        env: env_stamps,
        outputs: output_stamps,
    };
    let encoded = serde_json::to_vec(&material)
        .map_err(|error| RunnerError::Ui(format!("failed to encode cache fingerprint: {error}")))?;
    Ok(CacheFingerprintSnapshot {
        fingerprint: fnv1a_hex(&encoded),
        outputs_exist,
    })
}

fn collect_input_stamps(
    declarations: &[String],
    catalog_root: &Path,
) -> Result<Vec<DeclaredInputStamp>, RunnerError> {
    declarations
        .iter()
        .map(|declaration| {
            let matches = resolve_declared_matches(catalog_root, declaration)?;
            let mut stamped = Vec::with_capacity(matches.len());
            for path in matches {
                stamped.push(stamp_path(catalog_root, &path)?);
            }
            stamped.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(DeclaredInputStamp {
                declaration: declaration.clone(),
                matches: stamped,
            })
        })
        .collect()
}

fn collect_output_stamps(
    declarations: &[String],
    catalog_root: &Path,
) -> Result<Vec<DeclaredOutputStamp>, RunnerError> {
    declarations
        .iter()
        .map(|declaration| {
            let matches = resolve_declared_matches(catalog_root, declaration)?;
            let matched = matches
                .iter()
                .map(|path| render_relative_or_absolute(catalog_root, path))
                .collect::<Vec<String>>();
            let exists = if has_glob_magic(declaration) {
                !matched.is_empty()
            } else {
                catalog_root.join(declaration).exists()
            };
            Ok(DeclaredOutputStamp {
                declaration: declaration.clone(),
                exists,
                matched,
            })
        })
        .collect()
}

fn collect_env_stamps(keys: &[String]) -> Vec<DeclaredEnvStamp> {
    let mut env = keys
        .iter()
        .map(|key| DeclaredEnvStamp {
            key: key.clone(),
            value: std::env::var(key).ok(),
        })
        .collect::<Vec<DeclaredEnvStamp>>();
    env.sort_by(|a, b| a.key.cmp(&b.key));
    env
}

fn resolve_declared_matches(
    catalog_root: &Path,
    declaration: &str,
) -> Result<Vec<PathBuf>, RunnerError> {
    if has_glob_magic(declaration) {
        return resolve_glob_matches(catalog_root, declaration);
    }
    Ok(vec![catalog_root.join(declaration)])
}

fn resolve_glob_matches(catalog_root: &Path, pattern: &str) -> Result<Vec<PathBuf>, RunnerError> {
    let glob = Glob::new(pattern).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "invalid cache declaration glob `{pattern}`: {error}"
        ))
    })?;
    let matcher = glob.compile_matcher();
    let mut matches = WalkDir::new(catalog_root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path == catalog_root {
                return None;
            }
            let relative = path.strip_prefix(catalog_root).ok()?;
            let relative_rendered = relative.to_string_lossy().replace('\\', "/");
            matcher
                .is_match(&relative_rendered)
                .then_some(path.to_path_buf())
        })
        .collect::<Vec<PathBuf>>();
    matches.sort();
    Ok(matches)
}

fn stamp_path(catalog_root: &Path, path: &Path) -> Result<PathStamp, RunnerError> {
    let rendered = render_relative_or_absolute(catalog_root, path);
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(PathStamp {
            path: rendered,
            kind: "missing",
            exists: false,
            size: None,
            modified_epoch_ms: None,
            digest: None,
        });
    };

    if metadata.is_file() {
        let body = fs::read(path).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "failed reading cache input {}: {error}",
                path.display()
            ))
        })?;
        return Ok(PathStamp {
            path: rendered,
            kind: "file",
            exists: true,
            size: Some(metadata.len()),
            modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
            digest: Some(fnv1a_hex(&body)),
        });
    }

    if metadata.is_dir() {
        let digest = digest_directory(path)?;
        return Ok(PathStamp {
            path: rendered,
            kind: "dir",
            exists: true,
            size: None,
            modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
            digest: Some(digest),
        });
    }

    Ok(PathStamp {
        path: rendered,
        kind: "other",
        exists: true,
        size: None,
        modified_epoch_ms: metadata_modified_epoch_ms(&metadata),
        digest: None,
    })
}

fn digest_directory(root: &Path) -> Result<String, RunnerError> {
    let mut hasher = Fnv1a64::new();
    for entry in WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let rel_rendered = relative.to_string_lossy().replace('\\', "/");
        hasher.update(rel_rendered.as_bytes());

        let Ok(metadata) = fs::metadata(path) else {
            continue;
        };
        if metadata.is_file() {
            hasher.update(b"f");
            let body = fs::read(path).map_err(|error| {
                RunnerError::TaskInvocation(format!(
                    "failed reading cache directory input {}: {error}",
                    path.display()
                ))
            })?;
            hasher.update(&body);
        } else if metadata.is_dir() {
            hasher.update(b"d");
        }
    }
    Ok(hasher.finish_hex())
}

fn metadata_modified_epoch_ms(metadata: &fs::Metadata) -> Option<u128> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
}

fn render_relative_or_absolute(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.display().to_string())
}

fn has_glob_magic(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[') || value.contains('{')
}

fn load_cache_store(workspace_root: &Path) -> Result<TaskCacheStore, RunnerError> {
    let path = workspace_root.join(CACHE_DIR).join(CACHE_STORE_FILE);
    if !path.exists() {
        return Ok(TaskCacheStore::default());
    }
    let raw = fs::read_to_string(&path).map_err(|error| RunnerError::TaskManifestRead {
        path: path.clone(),
        error,
    })?;
    serde_json::from_str::<TaskCacheStore>(&raw).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to parse task cache store {}: {error}",
            path.display()
        ))
    })
}

fn save_cache_store(workspace_root: &Path, store: &TaskCacheStore) -> Result<(), RunnerError> {
    let cache_root = workspace_root.join(CACHE_DIR);
    fs::create_dir_all(&cache_root).map_err(|error| RunnerError::TaskManifestRead {
        path: cache_root.clone(),
        error,
    })?;
    let path = cache_root.join(CACHE_STORE_FILE);
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

fn now_epoch_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(bytes);
    hasher.finish_hex()
}

struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finish_hex(&self) -> String {
        format!("{:016x}", self.state)
    }
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
