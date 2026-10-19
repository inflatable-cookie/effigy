//! Managed-profile facts of a declared task, for `effigy tasks --resolve`.
//! Selector resolution itself has one implementation, in the runner.

use effigy_manifest::ManifestTask;

const DEFAULT_MANAGED_PROFILE: &str = "default";

fn has_concurrent_schema(task: &ManifestTask) -> bool {
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

pub fn has_concurrent_profile(task: &ManifestTask, profile_name: &str) -> bool {
    if task
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.concurrent_entries())
        .is_some()
    {
        return true;
    }
    profile_name == DEFAULT_MANAGED_PROFILE
        && has_concurrent_schema(task)
        && !task.concurrent.is_empty()
}
