use serde_json::json;

use super::catalog::select_catalog_and_task;
use super::util::parse_task_reference_invocation;
use super::{LoadedCatalog, ManifestTask, RunnerError, BUILTIN_TASKS};

pub(super) fn build_resolve_probe(
    raw_selector: Option<String>,
    catalogs: &[LoadedCatalog],
) -> Result<Option<serde_json::Value>, RunnerError> {
    let Some(raw_selector) = raw_selector else {
        return Ok(None);
    };
    let (selector, selector_args) = parse_task_reference_invocation(&raw_selector)?;
    let selector_task_name = selector.task_name.clone();
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;

    let probe = match select_catalog_and_task(&selector, catalogs, &cwd) {
        Ok(selection) => {
            if selection.task.mode.as_deref() == Some("tui") {
                let profile_name = selector_args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "default".to_owned());
                if !concurrent_entries_for_profile(selection.task, &profile_name) {
                    let available = available_concurrent_profiles(selection.task);
                    let available_display = if available.is_empty() {
                        "<none>".to_owned()
                    } else {
                        available.join(", ")
                    };
                    json!({
                        "selector": raw_selector,
                        "status": "error",
                        "catalog": serde_json::Value::Null,
                        "catalog_root": serde_json::Value::Null,
                        "task": selector_task_name,
                        "lock_scopes": lock_scopes_for_task(
                            &selector_task_name,
                            selection.task,
                            Some(&profile_name)
                        ),
                        "evidence": Vec::<String>::new(),
                        "error": format!(
                            "managed profile `{profile_name}` not found for task `{}`; available: {}",
                            selector_task_name,
                            available_display
                        ),
                    })
                } else {
                    let mut evidence = selection.evidence.clone();
                    evidence.push(format!(
                        "managed profile `{profile_name}` resolved via invocation `{raw_selector}`"
                    ));
                    json!({
                        "selector": raw_selector,
                        "status": "ok",
                        "catalog": selection.catalog.alias,
                        "catalog_root": selection.catalog.catalog_root.display().to_string(),
                        "task": selector_task_name,
                        "lock_scopes": lock_scopes_for_task(
                            &selector_task_name,
                            selection.task,
                            Some(&profile_name)
                        ),
                        "evidence": evidence,
                        "error": serde_json::Value::Null,
                    })
                }
            } else {
                json!({
                    "selector": raw_selector,
                    "status": "ok",
                    "catalog": selection.catalog.alias,
                    "catalog_root": selection.catalog.catalog_root.display().to_string(),
                    "task": selector_task_name,
                    "lock_scopes": lock_scopes_for_task(&selector_task_name, selection.task, None),
                    "evidence": selection.evidence,
                    "error": serde_json::Value::Null,
                })
            }
        }
        Err(error) => {
            if BUILTIN_TASKS
                .iter()
                .any(|(name, _)| *name == selector_task_name.as_str())
                || selector_task_name == "catalogs"
            {
                json!({
                    "selector": raw_selector,
                    "status": "ok",
                    "catalog": serde_json::Value::Null,
                    "catalog_root": serde_json::Value::Null,
                    "task": selector_task_name.clone(),
                    "lock_scopes": Vec::<String>::new(),
                    "evidence": vec![format!("resolved built-in task `{}`", selector_task_name)],
                    "error": serde_json::Value::Null,
                })
            } else {
                json!({
                    "selector": raw_selector,
                    "status": "error",
                    "catalog": serde_json::Value::Null,
                    "catalog_root": serde_json::Value::Null,
                    "task": selector_task_name,
                    "lock_scopes": Vec::<String>::new(),
                    "evidence": Vec::<String>::new(),
                    "error": error.to_string(),
                })
            }
        }
    };

    Ok(Some(probe))
}

fn concurrent_entries_for_profile(task: &ManifestTask, profile_name: &str) -> bool {
    if task
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.concurrent_entries())
        .is_some()
    {
        return true;
    }
    profile_name == "default" && !task.concurrent.is_empty()
}

fn available_concurrent_profiles(task: &ManifestTask) -> Vec<String> {
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
    if !task.concurrent.is_empty() && !available.iter().any(|name| name == "default") {
        available.push("default".to_owned());
    }
    available.sort();
    available
}

fn lock_scopes_for_task(
    task_name: &str,
    task: &ManifestTask,
    profile: Option<&str>,
) -> Vec<String> {
    let mut scopes = vec!["workspace".to_owned(), format!("task:{task_name}")];
    if task.mode.as_deref() == Some("tui") {
        let profile_name = profile.unwrap_or("default");
        scopes.push(format!("profile:{task_name}/{profile_name}"));
    }
    scopes
}
