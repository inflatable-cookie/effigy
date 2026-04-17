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

fn requested_profile_name(runtime_args: &TaskRuntimeArgs) -> &str {
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
