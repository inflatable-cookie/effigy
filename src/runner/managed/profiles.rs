use crate::runner::{ManifestManagedConcurrentEntry, ManifestTask};

pub(super) const DEFAULT_MANAGED_PROFILE: &str = "default";

pub(super) fn concurrent_entries_for_profile<'a>(
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

pub(super) fn has_concurrent_schema(task: &ManifestTask) -> bool {
    !task.concurrent.is_empty()
        || task
            .profiles
            .values()
            .any(|profile| profile.concurrent_entries().is_some())
}

pub(super) fn available_concurrent_profiles(task: &ManifestTask) -> Vec<String> {
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
