use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use effigy_cli::TaskInvocation;

use effigy_core::shell::{shell_quote, with_local_node_bin_path};

use super::policy::DEFER_DEPTH_ENV;
use super::trace::render_deferral_trace;
use crate::runner::error::RunnerError;
use effigy_manifest::DeferredCommand;
use effigy_tasks::TaskRuntimeArgs;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ComposerHomeCacheKey {
    composer_path: PathBuf,
    composer_home_env: Option<String>,
    home_env: Option<String>,
}

fn composer_home_cache() -> &'static Mutex<HashMap<ComposerHomeCacheKey, PathBuf>> {
    static CACHE: OnceLock<Mutex<HashMap<ComposerHomeCacheKey, PathBuf>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const IMPLICIT_DEFERRAL_CACHE_FILE: &str = "implicit-deferral-v1.json";

#[derive(Debug, Serialize, Deserialize)]
struct ImplicitDeferralCacheStore {
    #[serde(default = "implicit_deferral_cache_schema")]
    schema: String,
    #[serde(default = "implicit_deferral_cache_schema_version")]
    schema_version: u8,
    #[serde(default)]
    entries: BTreeMap<String, String>,
}

pub(in crate::runner) fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
) -> Result<String, RunnerError> {
    let current_depth = std::env::var(DEFER_DEPTH_ENV)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    if current_depth >= 1 {
        return Err(RunnerError::DeferLoopDetected {
            depth: current_depth,
        });
    }

    let command = build_deferred_command(task, runtime_args, deferral)?;

    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "sh".to_owned());
    let shell_arg = if shell.ends_with("zsh") || shell.ends_with("bash") {
        "-ic"
    } else {
        "-c"
    };
    let mut process = ProcessCommand::new(&shell);
    process
        .arg(shell_arg)
        .arg(&command)
        .current_dir(&deferral.working_dir)
        .env(DEFER_DEPTH_ENV, (current_depth + 1).to_string());
    with_local_node_bin_path(&mut process, &deferral.working_dir);
    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.clone(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            return Ok(render_deferral_trace(task, deferral, &command, cause));
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command,
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn build_deferred_command(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
) -> Result<String, RunnerError> {
    let args_rendered = runtime_args.passthrough.join(" ");
    let request_rendered = task.name.clone();
    let repo_rendered = shell_quote(&deferral.working_dir.display().to_string());
    let mut template = deferral.template.clone();
    if template.contains("{composer_global_effigy}") {
        let legacy_effigy = resolve_composer_global_effigy_path(&deferral.working_dir)?;
        template = template.replace(
            "{composer_global_effigy}",
            &shell_quote(&legacy_effigy.display().to_string()),
        );
    }
    Ok(template
        .replace("{request}", &request_rendered)
        .replace("{args}", &args_rendered)
        .replace("{repo}", &repo_rendered))
}

fn resolve_composer_global_effigy_path(workspace_root: &Path) -> Result<PathBuf, RunnerError> {
    let composer_path = resolve_path_executable("composer")?;
    let cache_key = ComposerHomeCacheKey {
        composer_path: composer_path.clone(),
        composer_home_env: std::env::var("COMPOSER_HOME").ok(),
        home_env: std::env::var("HOME").ok(),
    };

    if let Some(cached) = composer_home_cache()
        .lock()
        .expect("composer home cache poisoned")
        .get(&cache_key)
        .cloned()
    {
        return Ok(cached);
    }

    let persistent_key = composer_home_cache_store_key(&cache_key);
    if let Some(cached) = load_persisted_composer_home_cache(workspace_root, &persistent_key) {
        composer_home_cache()
            .lock()
            .expect("composer home cache poisoned")
            .insert(cache_key, cached.clone());
        return Ok(cached);
    }

    let output = ProcessCommand::new(&composer_path)
        .args(["global", "config", "home", "--absolute"])
        .output()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "composer global config home --absolute".to_owned(),
            error,
        })?;
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: "composer global config home --absolute".to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let home = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if home.is_empty() {
        return Err(RunnerError::task_invocation(
            "composer global config home --absolute returned an empty path",
        ));
    }

    let legacy_effigy = PathBuf::from(home).join("vendor/bin/effigy");
    composer_home_cache()
        .lock()
        .expect("composer home cache poisoned")
        .insert(cache_key, legacy_effigy.clone());
    save_persisted_composer_home_cache(workspace_root, &persistent_key, &legacy_effigy);
    Ok(legacy_effigy)
}

fn resolve_path_executable(name: &str) -> Result<PathBuf, RunnerError> {
    let path_value = std::env::var_os("PATH")
        .ok_or_else(|| RunnerError::task_invocation(format!("`{name}` requires PATH to be set")))?;
    for dir in std::env::split_paths(&path_value) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(RunnerError::task_invocation(format!(
        "implicit deferral requires `{name}` on PATH"
    )))
}

fn composer_home_cache_store_key(key: &ComposerHomeCacheKey) -> String {
    format!(
        "composer={}\ncomposer_home={}\nhome={}",
        key.composer_path.display(),
        key.composer_home_env.as_deref().unwrap_or(""),
        key.home_env.as_deref().unwrap_or(""),
    )
}

fn load_persisted_composer_home_cache(workspace_root: &Path, key: &str) -> Option<PathBuf> {
    let path = implicit_deferral_cache_path(workspace_root);
    let raw = fs::read_to_string(&path).ok()?;
    let store = serde_json::from_str::<ImplicitDeferralCacheStore>(&raw).ok()?;
    let cached = PathBuf::from(store.entries.get(key)?);
    cached.is_file().then_some(cached)
}

fn save_persisted_composer_home_cache(workspace_root: &Path, key: &str, value: &Path) {
    let cache_root = workspace_root.join(".effigy/cache");
    if fs::create_dir_all(&cache_root).is_err() {
        return;
    }

    let path = implicit_deferral_cache_path(workspace_root);
    let mut store = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<ImplicitDeferralCacheStore>(&raw).ok())
        .unwrap_or_default();
    store
        .entries
        .insert(key.to_owned(), value.display().to_string());
    let Ok(encoded) = serde_json::to_string_pretty(&store) else {
        return;
    };
    let _ = fs::write(path, encoded);
}

fn implicit_deferral_cache_path(workspace_root: &Path) -> PathBuf {
    workspace_root
        .join(".effigy/cache")
        .join(IMPLICIT_DEFERRAL_CACHE_FILE)
}

fn implicit_deferral_cache_schema() -> String {
    "effigy.deferral.implicit-cache.v1".to_owned()
}

fn implicit_deferral_cache_schema_version() -> u8 {
    1
}

impl Default for ImplicitDeferralCacheStore {
    fn default() -> Self {
        Self {
            schema: implicit_deferral_cache_schema(),
            schema_version: implicit_deferral_cache_schema_version(),
            entries: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
pub(crate) fn reset_composer_home_cache_for_tests() {
    composer_home_cache()
        .lock()
        .expect("composer home cache poisoned")
        .clear();
}
