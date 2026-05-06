use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use effigy_bootstrap::BootstrapStagedDbSeed;
use effigy_cli::BootstrapDbSeedInput;
use effigy_execution::ExecutionSurface;
use effigy_manifest::TASK_MANIFEST_FILE;
use serde_json::json;

use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, ActivationRequest,
};
use crate::runner::execute::api::{
    resolve_execution_binding_resolution, run_manifest_task_with_surface_and_env,
};
use crate::runner::manifest::load_task_manifest;
use crate::runner::runtime_session_context::{with_runtime_session_context, RuntimeSessionContext};

use super::error::RunnerError;
use effigy_cli::TaskInvocation;

pub(in crate::runner) const DB_SEED_TASK: &str = "bootstrap:db-seed";
pub(in crate::runner) const DB_SEEDS_DIR: &str = ".effigy/local/db-seeds";
pub(in crate::runner) const DB_SEEDS_METADATA_FILE: &str = "_effigy-db-seeds.json";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE: &str =
    "_effigy-bootstrap-db-seeds.json";
pub(in crate::runner) const DB_SEEDS_DIR_ENV: &str = "EFFIGY_DB_SEEDS_DIR";
pub(in crate::runner) const DB_SEED_FILE_ENV: &str = "EFFIGY_DB_SEED_FILE";
pub(in crate::runner) const DB_SEED_COUNT_ENV: &str = "EFFIGY_DB_SEED_COUNT";
pub(in crate::runner) const DB_SEED_FILES_ENV: &str = "EFFIGY_DB_SEED_FILES";
pub(in crate::runner) const DB_SEEDS_JSON_ENV: &str = "EFFIGY_DB_SEEDS_JSON";
pub(in crate::runner) const DB_SEED_TARGET_ENV: &str = "EFFIGY_DB_SEED_TARGET";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEEDS_DIR_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEEDS_DIR";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_FILE_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_FILE";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_COUNT_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_COUNT";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_FILES_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_FILES";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEEDS_JSON_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEEDS_JSON";
pub(in crate::runner) const LEGACY_BOOTSTRAP_DB_SEED_TARGET_ENV: &str =
    "EFFIGY_BOOTSTRAP_DB_SEED_TARGET";

pub(in crate::runner) fn data_seed_runtime_session_context() -> RuntimeSessionContext {
    RuntimeSessionContext::default()
}

pub(in crate::runner) fn resolve_db_seed_input_paths(
    cwd: &Path,
    db_seeds: &[BootstrapDbSeedInput],
) -> Vec<BootstrapDbSeedInput> {
    db_seeds
        .iter()
        .map(|seed| BootstrapDbSeedInput {
            target: seed.target.clone(),
            path: if seed.path.is_absolute() {
                seed.path.clone()
            } else {
                cwd.join(&seed.path)
            },
        })
        .collect()
}

pub(in crate::runner) fn maybe_prompt_db_seed_inputs(
    repo_root: &Path,
    output_json: bool,
    no_prompt: bool,
    effective_db_seeds: &mut Vec<BootstrapDbSeedInput>,
    prompt_checked: &mut bool,
) -> Result<(), RunnerError> {
    if *prompt_checked
        || !effective_db_seeds.is_empty()
        || !should_prompt_db_seeds(output_json, no_prompt)
    {
        return Ok(());
    }

    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let Some(targets) = manifest.bundle.as_ref().and_then(bundle_database_targets) else {
        *prompt_checked = true;
        return Ok(());
    };
    if targets.is_empty() {
        *prompt_checked = true;
        return Ok(());
    }

    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    *effective_db_seeds =
        collect_db_seed_prompts_from_io(repo_root, &targets, &mut stdin, &mut stdout)?;
    *prompt_checked = true;
    Ok(())
}

pub(in crate::runner) fn should_prompt_db_seeds(output_json: bool, no_prompt: bool) -> bool {
    !output_json && !no_prompt && io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn prompt_yes_no<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
) -> Result<bool, RunnerError> {
    prompt_yes_no_with_default(input, output, prompt, true)
}

fn prompt_yes_no_with_default<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
    default: bool,
) -> Result<bool, RunnerError> {
    output
        .write_all(prompt.as_bytes())
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive bootstrap prompt: {error}"
            ))
        })?;
    let mut line = String::new();
    input.read_line(&mut line).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to read interactive bootstrap input: {error}"
        ))
    })?;
    let normalized = line.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(default);
    }
    Ok(normalized == "y" || normalized == "yes")
}

pub(in crate::runner) fn collect_db_seed_prompts_from_io<R: BufRead, W: Write>(
    repo_root: &Path,
    targets: &[String],
    input: &mut R,
    output: &mut W,
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    output
        .write_all(
            b"No --db-seed inputs were supplied.\nEnter a SQL dump path for each database, or leave blank to skip.\n\n",
        )
        .and_then(|_| output.flush())
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to render interactive bootstrap prompt: {error}"
            ))
        })?;

    let mut db_seeds = Vec::new();
    for target in targets {
        loop {
            write!(output, "{target}: ")
                .and_then(|_| output.flush())
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to render interactive bootstrap prompt: {error}"
                    ))
                })?;
            let mut line = String::new();
            input.read_line(&mut line).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to read interactive bootstrap input: {error}"
                ))
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            let path = PathBuf::from(trimmed);
            let normalized = if path.is_absolute() {
                path
            } else {
                repo_root.join(path)
            };
            if !normalized.is_file() {
                writeln!(
                    output,
                    "Path does not exist or is not a readable file: {}",
                    normalized.display()
                )
                .and_then(|_| output.flush())
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to render interactive bootstrap prompt: {error}"
                    ))
                })?;
                continue;
            }
            db_seeds.push(BootstrapDbSeedInput {
                target: Some(target.clone()),
                path: normalized,
            });
            break;
        }
    }

    if db_seeds.is_empty() {
        return Ok(db_seeds);
    }

    let prompt = if db_seeds.len() == 1 {
        "Continue with 1 database seed file? [Y/n]: ".to_owned()
    } else {
        format!(
            "Continue with {} database seed file(s)? [Y/n]: ",
            db_seeds.len()
        )
    };
    if !prompt_yes_no(input, output, &prompt)? {
        return Err(RunnerError::task_invocation(
            "bootstrap cancelled during interactive database seed prompt",
        ));
    }

    Ok(db_seeds)
}

pub(in crate::runner) fn stage_db_seed_files(
    repo_root: &Path,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<BootstrapStagedDbSeed>, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let resolved_seeds = resolve_db_seed_targets(repo_root, &manifest, db_seeds)?;
    let staging_dir = repo_root.join(DB_SEEDS_DIR);
    std::fs::create_dir_all(&staging_dir).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to create db seed directory {}: {error}",
            staging_dir.display()
        ))
    })?;

    let mut seen_names = BTreeSet::new();
    let mut staged = Vec::with_capacity(resolved_seeds.len());
    for seed in &resolved_seeds {
        let source = &seed.path;
        if !source.is_file() {
            return Err(RunnerError::task_invocation(format!(
                "db seed is not a readable file: {}",
                source.display()
            )));
        }
        let Some(file_name) = source.file_name() else {
            return Err(RunnerError::task_invocation(format!(
                "db seed path has no file name: {}",
                source.display()
            )));
        };
        let base_name = file_name.to_string_lossy().to_string();
        let staged_name = match seed.target.as_deref() {
            Some(target) => format!("{target}--{base_name}"),
            None => base_name.clone(),
        };
        if !seen_names.insert(staged_name.clone()) {
            return Err(RunnerError::task_invocation(format!(
                "duplicate staged db seed file name `{staged_name}`; rename one input or use distinct seed targets"
            )));
        }
        let destination = staging_dir.join(&staged_name);
        std::fs::copy(source, &destination).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to stage db seed {} -> {}: {error}",
                source.display(),
                destination.display()
            ))
        })?;
        staged.push(BootstrapStagedDbSeed {
            target: seed.target.clone(),
            source_path: source.clone(),
            staged_path: destination,
        });
    }

    let metadata = serde_json::to_string(
        &staged
            .iter()
            .map(|seed| {
                json!({
                    "target": seed.target,
                    "source_path": seed.source_path.display().to_string(),
                    "staged_path": Path::new(DB_SEEDS_DIR)
                        .join(
                            seed.staged_path
                                .file_name()
                                .expect("staged seed file should have name"),
                        )
                        .display()
                        .to_string(),
                })
            })
            .collect::<Vec<_>>(),
    )
    .expect("db seed metadata should serialize");

    for file_name in [
        DB_SEEDS_METADATA_FILE,
        LEGACY_BOOTSTRAP_DB_SEEDS_METADATA_FILE,
    ] {
        std::fs::write(staging_dir.join(file_name), &metadata).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write db seed metadata file {}: {error}",
                staging_dir.join(file_name).display()
            ))
        })?;
    }
    Ok(staged)
}

pub(in crate::runner) fn db_seed_env(
    staged_db_seeds: &[BootstrapStagedDbSeed],
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if staged_db_seeds.is_empty() {
        return env;
    }
    let seeds_dir = Path::new(DB_SEEDS_DIR);
    let metadata_json = serde_json::to_string(
        &staged_db_seeds
            .iter()
            .map(|seed| {
                json!({
                    "target": seed.target,
                    "source_path": seed.source_path.display().to_string(),
                    "staged_path": seeds_dir
                        .join(
                            seed.staged_path
                                .file_name()
                                .expect("staged seed file should have name"),
                        )
                        .display()
                        .to_string(),
                })
            })
            .collect::<Vec<_>>(),
    )
    .expect("db seed env json should serialize");
    let files = staged_db_seeds
        .iter()
        .map(|seed| {
            seeds_dir
                .join(
                    seed.staged_path
                        .file_name()
                        .expect("staged seed file should have name"),
                )
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();

    for key in [DB_SEEDS_DIR_ENV, LEGACY_BOOTSTRAP_DB_SEEDS_DIR_ENV] {
        env.insert(key.to_owned(), seeds_dir.display().to_string());
    }
    for key in [DB_SEED_COUNT_ENV, LEGACY_BOOTSTRAP_DB_SEED_COUNT_ENV] {
        env.insert(key.to_owned(), staged_db_seeds.len().to_string());
    }
    for key in [DB_SEED_FILES_ENV, LEGACY_BOOTSTRAP_DB_SEED_FILES_ENV] {
        env.insert(key.to_owned(), files.join("\n"));
    }
    for key in [DB_SEEDS_JSON_ENV, LEGACY_BOOTSTRAP_DB_SEEDS_JSON_ENV] {
        env.insert(key.to_owned(), metadata_json.clone());
    }

    if staged_db_seeds.len() == 1 {
        let staged_file = seeds_dir
            .join(
                staged_db_seeds[0]
                    .staged_path
                    .file_name()
                    .expect("staged seed file should have name"),
            )
            .display()
            .to_string();
        for key in [DB_SEED_FILE_ENV, LEGACY_BOOTSTRAP_DB_SEED_FILE_ENV] {
            env.insert(key.to_owned(), staged_file.clone());
        }
        if let Some(target) = staged_db_seeds[0].target.as_deref() {
            for key in [DB_SEED_TARGET_ENV, LEGACY_BOOTSTRAP_DB_SEED_TARGET_ENV] {
                env.insert(key.to_owned(), target.to_owned());
            }
        }
    }
    env
}

pub(in crate::runner) fn run_db_seed_task(
    repo_root: &Path,
    env_overrides: &BTreeMap<String, String>,
) -> Result<(), RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    if !manifest.tasks.contains_key(DB_SEED_TASK) {
        return Err(RunnerError::task_invocation(format!(
            "database seed input was supplied but {} does not define task `{DB_SEED_TASK}`",
            repo_root.join(TASK_MANIFEST_FILE).display()
        )));
    }
    prepare_db_seed_runtime(repo_root, &manifest)?;
    with_runtime_session_context(data_seed_runtime_session_context(), || {
        run_manifest_task_with_surface_and_env(
            &TaskInvocation {
                name: DB_SEED_TASK.to_owned(),
                args: Vec::new(),
            },
            repo_root.to_path_buf(),
            ExecutionSurface::DataSeed,
            env_overrides,
        )
        .map(|_| ())
        .map_err(|err| {
            RunnerError::task_invocation(format!(
                "database seed task `{DB_SEED_TASK}` failed: {err}"
            ))
        })
    })
}

fn prepare_db_seed_runtime(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
) -> Result<(), RunnerError> {
    let Some(task) = manifest.tasks.get(DB_SEED_TASK) else {
        return Ok(());
    };
    let binding_resolution = resolve_execution_binding_resolution(
        manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        DB_SEED_TASK,
        task,
        "database seed",
    )?;
    let Some(policy) = binding_resolution.effective_policy(repo_root)? else {
        return Ok(());
    };
    activate_container_runtime_for_task(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: binding_resolution.binding().container_name(),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: data_seed_runtime_session_context(),
        },
    )?;
    Ok(())
}

fn resolve_db_seed_targets(
    repo_root: &Path,
    manifest: &effigy_manifest::TaskManifest,
    db_seeds: &[BootstrapDbSeedInput],
) -> Result<Vec<BootstrapDbSeedInput>, RunnerError> {
    let declared_targets = manifest.bundle.as_ref().and_then(bundle_database_targets);

    let mut seen_targets = BTreeSet::new();
    let mut resolved = Vec::with_capacity(db_seeds.len());
    for seed in db_seeds {
        let effective_target = match seed.target.as_deref() {
            Some(target) => {
                if let Some(declared_targets) = declared_targets.as_ref() {
                    if !declared_targets.iter().any(|declared| declared == target) {
                        return Err(RunnerError::task_invocation(format!(
                            "db seed target `{target}` is not declared in `[bundle].databases` for {}; valid targets: {}",
                            repo_root.join(TASK_MANIFEST_FILE).display(),
                            declared_targets.join(", ")
                        )));
                    }
                }
                Some(target.to_owned())
            }
            None => match declared_targets.as_ref() {
                Some(declared_targets) if declared_targets.len() == 1 => {
                    Some(declared_targets[0].clone())
                }
                Some(declared_targets) if declared_targets.len() > 1 => {
                    return Err(RunnerError::task_invocation(format!(
                        "db seed input `{}` must name a target because `[bundle].databases` declares multiple databases: {}",
                        seed.path.display(),
                        declared_targets.join(", ")
                    )));
                }
                _ => None,
            },
        };
        if let Some(target) = effective_target.as_deref() {
            if !seen_targets.insert(target.to_owned()) {
                return Err(RunnerError::task_invocation(format!(
                    "duplicate db seed target `{target}`"
                )));
            }
        }
        resolved.push(BootstrapDbSeedInput {
            target: effective_target,
            path: seed.path.clone(),
        });
    }
    Ok(resolved)
}

pub(in crate::runner) fn bundle_database_targets(
    bundle: &effigy_manifest::ManifestBundleConfig,
) -> Option<Vec<String>> {
    let value = bundle
        .inputs
        .get("databases")
        .or_else(|| bundle.inputs.get("database"))?;

    match value {
        toml::Value::Array(values) => {
            let targets = values
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if targets.is_empty() {
                None
            } else {
                Some(targets)
            }
        }
        toml::Value::String(value) => {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(vec![value.to_owned()])
            }
        }
        _ => None,
    }
}

fn bootstrap_env_override_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(in crate::runner) struct ScopedDbSeedEnvOverride {
    _guard: MutexGuard<'static, ()>,
    original: Vec<(String, Option<OsString>)>,
}

impl ScopedDbSeedEnvOverride {
    pub(in crate::runner) fn set(entries: &BTreeMap<String, String>) -> Self {
        let guard = bootstrap_env_override_lock()
            .lock()
            .expect("db seed env override mutex should not be poisoned");
        let mut original = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            original.push((key.clone(), std::env::var_os(key)));
            unsafe {
                std::env::set_var(key, value);
            }
        }
        Self {
            _guard: guard,
            original,
        }
    }
}

impl Drop for ScopedDbSeedEnvOverride {
    fn drop(&mut self) {
        for (key, value) in self.original.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}
