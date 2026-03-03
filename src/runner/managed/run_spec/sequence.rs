use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use crate::runner::manifest::{ManifestEnvEntry, ManifestEnvFileDirective};

use super::super::super::{LoadedCatalog, ManifestManagedRunStep, RunnerError};
use super::super::scheduler;
use super::command::wrap_command_with_task_env;
use super::run_step::resolve_task_run_step;

pub(super) fn render_run_sequence(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    task_env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if steps.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` has an empty run array"
        )));
    }
    let mut commands = Vec::with_capacity(steps.len());
    let mut policies = Vec::with_capacity(steps.len());
    let mut chained_env = BTreeMap::<String, String>::new();
    let mut current_env_files = normalize_env_file_directive(task_env_file, "task env_file")?;
    let mut dotenv_cache = BTreeMap::<PathBuf, BTreeMap<String, String>>::new();
    for step in steps {
        if let ManifestManagedRunStep::Step(table) = step {
            apply_run_step_env(
                task_name,
                table.env.as_ref(),
                table.env_file.as_ref(),
                env_profiles,
                repo_root,
                catalogs,
                &mut chained_env,
                &mut current_env_files,
                &mut dotenv_cache,
            )?;
        }
        let command = resolve_task_run_step(
            task_name,
            step,
            args_rendered,
            repo_root,
            catalogs,
            task_scope_cwd,
            depth,
        )?;
        commands.push(wrap_command_with_task_env(command, &chained_env, repo_root));
        policies.push(scheduler::step_policy_for(step));
    }
    let has_non_default_policy = policies.iter().copied().any(|policy| !policy.is_default());
    let schedule = scheduler::build_run_sequence_schedule(task_name, steps)?;
    match schedule {
        Some(levels) => Ok(scheduler::render_parallel_run_levels_with_policy(
            &commands, &levels, &policies,
        )),
        None if has_non_default_policy => {
            let sequential_levels = (0..commands.len())
                .map(|index| vec![index])
                .collect::<Vec<Vec<usize>>>();
            Ok(scheduler::render_parallel_run_levels_with_policy(
                &commands,
                &sequential_levels,
                &policies,
            ))
        }
        None => Ok(commands.join(" && ")),
    }
}

fn apply_run_step_env(
    task_name: &str,
    env: Option<&crate::runner::manifest::ManifestRunStepEnv>,
    env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    chained_env: &mut BTreeMap<String, String>,
    current_env_files: &mut Option<Vec<String>>,
    dotenv_cache: &mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<(), RunnerError> {
    if let Some(env_file) = env_file {
        *current_env_files = normalize_env_file_directive(Some(env_file), "run step env_file")?;
    }
    let Some(env) = env else {
        return Ok(());
    };
    match env {
        crate::runner::manifest::ManifestRunStepEnv::Inline(table) => {
            for (key, value) in table {
                chained_env.insert(key.clone(), value.clone());
            }
            Ok(())
        }
        crate::runner::manifest::ManifestRunStepEnv::Profile(profile_name_raw) => {
            let profile_name = profile_name_raw.trim();
            if profile_name.is_empty() {
                return Err(RunnerError::TaskInvocation(format!(
                    "task `{task_name}` run step is invalid: env profile name cannot be empty"
                )));
            }
            if let Some((resolved_key, profile)) =
                resolve_manifest_env_entry(profile_name, env_profiles, repo_root, catalogs)
            {
                match profile {
                    ManifestEnvEntry::Value(value) => {
                        chained_env.insert(resolved_key, value.clone());
                    }
                    ManifestEnvEntry::Profile(entries) => {
                        for entry in entries {
                            for (key, value) in entry {
                                chained_env.insert(key.clone(), value.clone());
                            }
                        }
                    }
                }
                return Ok(());
            }
            if let Some((resolved_key, value)) = resolve_process_env_entry(profile_name) {
                chained_env.insert(resolved_key, value);
                return Ok(());
            }
            if let Some((resolved_key, value)) =
                resolve_dotenv_env_entry(
                    profile_name,
                    repo_root,
                    catalogs,
                    current_env_files.as_deref(),
                    dotenv_cache,
                )?
            {
                chained_env.insert(resolved_key, value);
                return Ok(());
            }
            Err(unknown_env_entry_error(task_name, profile_name))
        }
    }
}

fn resolve_manifest_env_entry<'a>(
    entry_ref: &str,
    local_env_entries: &'a BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &'a [LoadedCatalog],
) -> Option<(String, &'a ManifestEnvEntry)> {
    if let Some(local) = local_env_entries.get(entry_ref) {
        return Some((entry_ref.to_owned(), local));
    }
    let Some((catalog_path, env_key)) = split_catalog_env_reference(entry_ref) else {
        return None;
    };
    let target_catalog_root = resolve_catalog_reference_root(catalog_path, repo_root);
    let target_catalog = catalogs
        .iter()
        .find(|catalog| normalize_path(&catalog.catalog_root) == target_catalog_root)?;
    let entry = target_catalog.manifest.env.get(env_key)?;
    Some((env_key.to_owned(), entry))
}

fn resolve_process_env_entry(entry_ref: &str) -> Option<(String, String)> {
    if split_catalog_env_reference(entry_ref).is_some() {
        return None;
    }
    std::env::var(entry_ref)
        .ok()
        .map(|value| (entry_ref.to_owned(), value))
}

fn resolve_dotenv_env_entry(
    entry_ref: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    env_files: Option<&[String]>,
    dotenv_cache: &mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<Option<(String, String)>, RunnerError> {
    if let Some((catalog_path, env_key)) = split_catalog_env_reference(entry_ref) {
        let target_catalog_root = resolve_catalog_reference_root(catalog_path, repo_root);
        let Some(target_catalog) = catalogs
            .iter()
            .find(|catalog| normalize_path(&catalog.catalog_root) == target_catalog_root)
        else {
            return Ok(None);
        };
        let value = resolve_dotenv_entry_from_catalog(
            &target_catalog.catalog_root,
            env_key,
            env_files,
            dotenv_cache,
        )?;
        return Ok(value.map(|value| (env_key.to_owned(), value)));
    }
    let value = resolve_dotenv_entry_from_catalog(repo_root, entry_ref, env_files, dotenv_cache)?;
    Ok(value.map(|value| (entry_ref.to_owned(), value)))
}

fn resolve_dotenv_entry_from_catalog(
    catalog_root: &Path,
    key: &str,
    env_files: Option<&[String]>,
    dotenv_cache: &mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<Option<String>, RunnerError> {
    let normalized_root = normalize_path(catalog_root);
    let env_file_paths = resolve_env_file_paths(&normalized_root, env_files);
    for env_file_path in env_file_paths {
        let entries = load_dotenv_entries_for_path(&env_file_path, dotenv_cache)?;
        if let Some(value) = entries.get(key) {
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

fn load_dotenv_entries_for_path<'a>(
    env_file_path: &Path,
    dotenv_cache: &'a mut BTreeMap<PathBuf, BTreeMap<String, String>>,
) -> Result<&'a BTreeMap<String, String>, RunnerError> {
    let env_file_path = normalize_path(env_file_path);
    if !dotenv_cache.contains_key(&env_file_path) {
        let parsed = parse_dotenv_file(&env_file_path)?;
        dotenv_cache.insert(env_file_path.clone(), parsed);
    }
    Ok(dotenv_cache
        .get(&env_file_path)
        .expect("dotenv cache entry should exist"))
}

fn parse_dotenv_file(env_file: &Path) -> Result<BTreeMap<String, String>, RunnerError> {
    let src = match fs::read_to_string(env_file) {
        Ok(src) => src,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(RunnerError::TaskInvocation(format!(
                "failed to read env file `{}`: {error}",
                env_file.display()
            )));
        }
    };
    let mut entries = BTreeMap::new();
    for raw_line in src.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(exported) = line.strip_prefix("export ") {
            line = exported.trim_start();
        }
        let Some((key_raw, value_raw)) = line.split_once('=') else {
            continue;
        };
        let key = key_raw.trim();
        if key.is_empty() {
            continue;
        }
        let value = strip_matching_quotes(value_raw.trim());
        entries.insert(key.to_owned(), value.to_owned());
    }
    Ok(entries)
}

fn strip_matching_quotes(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn normalize_env_file_directive(
    env_file: Option<&ManifestEnvFileDirective>,
    field_label: &str,
) -> Result<Option<Vec<String>>, RunnerError> {
    let Some(env_file) = env_file else {
        return Ok(None);
    };
    let entries = match env_file {
        ManifestEnvFileDirective::Single(value) => vec![normalize_env_file_entry(
            value,
            field_label,
            None,
        )?],
        ManifestEnvFileDirective::Many(values) => {
            if values.is_empty() {
                return Err(RunnerError::TaskInvocation(format!(
                    "{field_label} is invalid: array cannot be empty"
                )));
            }
            let mut normalized = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                normalized.push(normalize_env_file_entry(value, field_label, Some(index))?);
            }
            normalized
        }
    };
    Ok(Some(entries))
}

fn normalize_env_file_entry(
    value: &str,
    field_label: &str,
    index: Option<usize>,
) -> Result<String, RunnerError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        let suffix = index
            .map(|idx| format!("[{idx}]"))
            .unwrap_or_else(String::new);
        return Err(RunnerError::TaskInvocation(format!(
            "{field_label}{suffix} is invalid: value cannot be empty"
        )));
    }
    Ok(normalized.to_owned())
}

fn resolve_env_file_paths(catalog_root: &Path, env_files: Option<&[String]>) -> Vec<PathBuf> {
    let defaults = vec![".env".to_owned()];
    let env_files = env_files.unwrap_or(defaults.as_slice());
    env_files
        .iter()
        .map(|env_file| {
            let resolved = if Path::new(env_file).is_absolute() {
                PathBuf::from(env_file)
            } else {
                catalog_root.join(env_file)
            };
            normalize_path(&resolved)
        })
        .collect::<Vec<PathBuf>>()
}

fn split_catalog_env_reference(entry_ref: &str) -> Option<(&str, &str)> {
    let split_at = entry_ref.rfind(['/', '\\'])?;
    let (catalog_path, env_key_with_sep) = entry_ref.split_at(split_at);
    let env_key = env_key_with_sep
        .strip_prefix('/')
        .or_else(|| env_key_with_sep.strip_prefix('\\'))?;
    if catalog_path.is_empty() || env_key.is_empty() {
        return None;
    }
    Some((catalog_path, env_key))
}

fn resolve_catalog_reference_root(catalog_path: &str, repo_root: &Path) -> PathBuf {
    let resolved = if Path::new(catalog_path).is_absolute() {
        PathBuf::from(catalog_path)
    } else {
        repo_root.join(catalog_path)
    };
    normalize_path(&resolved)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn unknown_env_entry_error(task_name: &str, entry_ref: &str) -> RunnerError {
    RunnerError::TaskInvocation(format!(
        "task `{task_name}` run step references unknown env entry `{entry_ref}`"
    ))
}
