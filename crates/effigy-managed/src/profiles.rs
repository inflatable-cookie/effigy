use super::plan;
use crate::ManagedError;
use effigy_manifest::{ManifestManagedConcurrentEntry, ManifestTask};
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

pub const DEFAULT_MANAGED_PROFILE: &str = "default";

pub struct ResolvedConcurrentProfile<'a> {
    pub profile_name: String,
    pub entries: &'a [ManifestManagedConcurrentEntry],
}

pub fn select_concurrent_profile<'a>(
    selector: &TaskSelector,
    task: &'a ManifestTask,
    runtime_args: &TaskRuntimeArgs,
) -> Result<ResolvedConcurrentProfile<'a>, ManagedError> {
    let profile_name = requested_profile_name(runtime_args);
    if let Some(profile) = resolve_concurrent_profile(task, profile_name) {
        return Ok(profile);
    }
    if has_concurrent_schema(task) {
        return Err(ManagedError::TaskManagedProfileNotFound {
            task: selector.task_name.clone(),
            profile: profile_name.to_owned(),
            available: available_concurrent_profiles(task),
        });
    }
    Err(plan::invalid_managed_process_definition(
        selector,
        "concurrent",
        "managed `mode = \"tui\"` requires `concurrent = [...]` in `[tasks.<name>]` (default profile) and/or `[tasks.<name>.profiles.<profile>]`",
    ))
}

/// The empty profile a managed task bound to a container environment runs
/// when it declares no `concurrent` entries: the session is the environment
/// (its host processes and their status) plus the task's own `run` when it
/// has one (effigy#322).
pub fn environment_only_profile<'a>(
    task: &'a ManifestTask,
    runtime_args: &TaskRuntimeArgs,
) -> Option<ResolvedConcurrentProfile<'a>> {
    environment_only_profile_for(task, requested_profile_name(runtime_args))
}

/// [`environment_only_profile`] by profile name, for the control surfaces.
pub fn environment_only_profile_for<'a>(
    task: &'a ManifestTask,
    profile_name: &str,
) -> Option<ResolvedConcurrentProfile<'a>> {
    if has_concurrent_schema(task) || profile_name != DEFAULT_MANAGED_PROFILE {
        return None;
    }
    Some(ResolvedConcurrentProfile {
        profile_name: DEFAULT_MANAGED_PROFILE.to_owned(),
        entries: &[],
    })
}

/// Whether `task` runs `profile_name`: an explicit profile, or the
/// environment-only session of a task bound to a container environment that
/// declares no `concurrent` entries (effigy#322).
pub fn has_managed_profile(
    task: &ManifestTask,
    profile_name: &str,
    environment_bound: bool,
) -> bool {
    resolve_concurrent_profile(task, profile_name).is_some()
        || (environment_bound && environment_only_profile_for(task, profile_name).is_some())
}

pub fn resolve_concurrent_profile<'a>(
    task: &'a ManifestTask,
    profile_name: &str,
) -> Option<ResolvedConcurrentProfile<'a>> {
    concurrent_entries_for_profile(task, profile_name).map(|entries| ResolvedConcurrentProfile {
        profile_name: profile_name.to_owned(),
        entries,
    })
}

pub fn has_concurrent_schema(task: &ManifestTask) -> bool {
    !task.concurrent.is_empty()
        || task
            .profiles
            .values()
            .any(|profile| profile.concurrent_entries().is_some())
}

pub fn available_concurrent_profiles(task: &ManifestTask) -> Vec<String> {
    let mut available = task
        .profiles
        .iter()
        .filter_map(|(name, profile)| {
            profile
                .concurrent_entries()
                .is_some()
                .then_some(name.clone())
        })
        .collect::<Vec<String>>();
    if !task.concurrent.is_empty() && !available.iter().any(|name| name == DEFAULT_MANAGED_PROFILE)
    {
        available.push(DEFAULT_MANAGED_PROFILE.to_owned());
    }
    available.sort();
    available
}

/// The profile selector used by managed plan resolution.
pub fn requested_profile_name(runtime_args: &TaskRuntimeArgs) -> &str {
    runtime_args
        .passthrough
        .first()
        .map(String::as_str)
        .unwrap_or(DEFAULT_MANAGED_PROFILE)
}

pub fn has_concurrent_profile(task: &ManifestTask, profile_name: &str) -> bool {
    resolve_concurrent_profile(task, profile_name).is_some()
}

fn concurrent_entries_for_profile<'a>(
    task: &'a ManifestTask,
    profile_name: &str,
) -> Option<&'a [ManifestManagedConcurrentEntry]> {
    if let Some(entries) = task
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.concurrent_entries())
    {
        return Some(entries);
    }
    if profile_name == DEFAULT_MANAGED_PROFILE && !task.concurrent.is_empty() {
        return Some(task.concurrent.as_slice());
    }
    None
}
