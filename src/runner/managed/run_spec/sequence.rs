use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::runner::manifest::ManifestEnvEntry;

use super::super::super::{LoadedCatalog, ManifestManagedRunStep, RunnerError};
use super::super::scheduler;
use super::command::wrap_command_with_task_env;
use super::run_step::resolve_task_run_step;

pub(super) fn render_run_sequence(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
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
    for step in steps {
        if let ManifestManagedRunStep::Step(table) = step {
            apply_run_step_env(
                task_name,
                table.env.as_ref(),
                env_profiles,
                repo_root,
                catalogs,
                &mut chained_env,
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
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    chained_env: &mut BTreeMap<String, String>,
) -> Result<(), RunnerError> {
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
            let (resolved_key, profile) = resolve_env_entry(
                task_name,
                profile_name,
                env_profiles,
                repo_root,
                catalogs,
            )?;
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
            Ok(())
        }
    }
}

fn resolve_env_entry<'a>(
    task_name: &str,
    entry_ref: &str,
    local_env_entries: &'a BTreeMap<String, ManifestEnvEntry>,
    repo_root: &Path,
    catalogs: &'a [LoadedCatalog],
) -> Result<(String, &'a ManifestEnvEntry), RunnerError> {
    if let Some(local) = local_env_entries.get(entry_ref) {
        return Ok((entry_ref.to_owned(), local));
    }
    let Some((catalog_path, env_key)) = split_catalog_env_reference(entry_ref) else {
        return Err(unknown_env_entry_error(task_name, entry_ref));
    };
    let target_catalog_root = resolve_catalog_reference_root(catalog_path, repo_root);
    let Some(target_catalog) = catalogs
        .iter()
        .find(|catalog| normalize_path(&catalog.catalog_root) == target_catalog_root)
    else {
        return Err(unknown_env_entry_error(task_name, entry_ref));
    };
    let Some(entry) = target_catalog.manifest.env.get(env_key) else {
        return Err(unknown_env_entry_error(task_name, entry_ref));
    };
    Ok((env_key.to_owned(), entry))
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
