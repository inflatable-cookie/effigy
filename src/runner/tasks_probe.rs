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
        Ok(selection) => build_selected_probe(
            &raw_selector,
            &selector_task_name,
            &selector_args,
            selection,
        ),
        Err(error) => {
            if BUILTIN_TASKS
                .iter()
                .any(|(name, _)| *name == selector_task_name.as_str())
                || selector_task_name == "catalogs"
            {
                probe_value(
                    &raw_selector,
                    "ok",
                    None,
                    None,
                    &selector_task_name,
                    Vec::new(),
                    vec![format!("resolved built-in task `{}`", selector_task_name)],
                    None,
                )
            } else {
                probe_value(
                    &raw_selector,
                    "error",
                    None,
                    None,
                    &selector_task_name,
                    Vec::new(),
                    Vec::new(),
                    Some(error.to_string()),
                )
            }
        }
    };

    Ok(Some(probe))
}

fn build_selected_probe(
    raw_selector: &str,
    selector_task_name: &str,
    selector_args: &[String],
    selection: super::TaskSelection<'_>,
) -> serde_json::Value {
    if selection.task.mode.as_deref() != Some("tui") {
        return probe_value(
            raw_selector,
            "ok",
            Some(selection.catalog.alias.clone()),
            Some(selection.catalog.catalog_root.display().to_string()),
            selector_task_name,
            lock_scopes_for_task(selector_task_name, selection.task, None),
            selection.evidence,
            None,
        );
    }

    let profile_name = selector_args
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_owned());
    let lock_scopes = lock_scopes_for_task(selector_task_name, selection.task, Some(&profile_name));
    if !concurrent_entries_for_profile(selection.task, &profile_name) {
        let available_display = render_available_profiles(selection.task);
        return probe_value(
            raw_selector,
            "error",
            None,
            None,
            selector_task_name,
            lock_scopes,
            Vec::new(),
            Some(format!(
                "managed profile `{profile_name}` not found for task `{}`; available: {}",
                selector_task_name, available_display
            )),
        );
    }

    let mut evidence = selection.evidence;
    evidence.push(format!(
        "managed profile `{profile_name}` resolved via invocation `{raw_selector}`"
    ));
    probe_value(
        raw_selector,
        "ok",
        Some(selection.catalog.alias.clone()),
        Some(selection.catalog.catalog_root.display().to_string()),
        selector_task_name,
        lock_scopes,
        evidence,
        None,
    )
}

fn render_available_profiles(task: &ManifestTask) -> String {
    let available = available_concurrent_profiles(task);
    if available.is_empty() {
        "<none>".to_owned()
    } else {
        available.join(", ")
    }
}

fn probe_value(
    selector: &str,
    status: &str,
    catalog: Option<String>,
    catalog_root: Option<String>,
    task: &str,
    lock_scopes: Vec<String>,
    evidence: Vec<String>,
    error: Option<String>,
) -> serde_json::Value {
    json!({
        "selector": selector,
        "status": status,
        "catalog": catalog,
        "catalog_root": catalog_root,
        "task": task,
        "lock_scopes": lock_scopes,
        "evidence": evidence,
        "error": error,
    })
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
