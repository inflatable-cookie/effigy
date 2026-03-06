use super::super::catalog::select_catalog_and_task;
use super::super::command_context::current_working_dir;
use super::super::managed::profiles::{
    available_concurrent_profiles, has_concurrent_profile, DEFAULT_MANAGED_PROFILE,
};
use super::super::manifest::task_runtime::ManifestTask;
use super::super::model::catalog::{LoadedCatalog, TaskSelection};
use super::super::model::constants::BUILTIN_TASKS;
use super::super::util::parse_task_reference_invocation;
use super::model::ResolveProbe;
use crate::runner::error::RunnerError;

pub(in crate::runner) fn build_resolve_probe(
    raw_selector: Option<String>,
    catalogs: &[LoadedCatalog],
) -> Result<Option<serde_json::Value>, RunnerError> {
    let Some(raw_selector) = raw_selector else {
        return Ok(None);
    };
    let (selector, selector_args) = parse_task_reference_invocation(&raw_selector)?;
    let selector_task_name = selector.task_name.clone();
    let cwd = current_working_dir()?;

    let probe = match select_catalog_and_task(&selector, catalogs, &cwd) {
        Ok(selection) => build_selected_probe(
            &raw_selector,
            &selector_task_name,
            &selector_args,
            selection,
        ),
        Err(_error) if is_builtin_or_catalogs_task(&selector_task_name) => ResolveProbe::ok(
            &raw_selector,
            &selector_task_name,
            None,
            None,
            Vec::new(),
            vec![format!("resolved built-in task `{selector_task_name}`")],
        )
        .into_json(),
        Err(error) => ResolveProbe::error(
            &raw_selector,
            &selector_task_name,
            Vec::new(),
            error.to_string(),
        )
        .into_json(),
    };

    Ok(Some(probe))
}

fn build_selected_probe(
    raw_selector: &str,
    selector_task_name: &str,
    selector_args: &[String],
    selection: TaskSelection<'_>,
) -> serde_json::Value {
    if selection.task.mode.as_deref() != Some("tui") {
        return ResolveProbe::ok(
            raw_selector,
            selector_task_name,
            Some(selection.catalog.alias.clone()),
            Some(selection.catalog.catalog_root.display().to_string()),
            lock_scopes_for_task(selector_task_name, selection.task, None),
            selection.evidence,
        )
        .into_json();
    }

    let profile_name = selector_args
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_MANAGED_PROFILE.to_owned());
    let lock_scopes = lock_scopes_for_task(selector_task_name, selection.task, Some(&profile_name));
    if !has_concurrent_profile(selection.task, &profile_name) {
        let available_display = render_available_profiles(selection.task);
        return ResolveProbe::error(
            raw_selector,
            selector_task_name,
            lock_scopes,
            format!(
                "managed profile `{profile_name}` not found for task `{}`; available: {}",
                selector_task_name, available_display
            ),
        )
        .into_json();
    }

    let mut evidence = selection.evidence;
    evidence.push(format!(
        "managed profile `{profile_name}` resolved via invocation `{raw_selector}`"
    ));
    ResolveProbe::ok(
        raw_selector,
        selector_task_name,
        Some(selection.catalog.alias.clone()),
        Some(selection.catalog.catalog_root.display().to_string()),
        lock_scopes,
        evidence,
    )
    .into_json()
}

fn render_available_profiles(task: &ManifestTask) -> String {
    let available = available_concurrent_profiles(task);
    if available.is_empty() {
        "<none>".to_owned()
    } else {
        available.join(", ")
    }
}

fn is_builtin_or_catalogs_task(task_name: &str) -> bool {
    BUILTIN_TASKS.iter().any(|(name, _)| *name == task_name) || task_name == "catalogs"
}

fn lock_scopes_for_task(
    task_name: &str,
    task: &ManifestTask,
    profile: Option<&str>,
) -> Vec<String> {
    let mut scopes = vec!["workspace".to_owned(), format!("task:{task_name}")];
    if task.mode.as_deref() == Some("tui") {
        let profile_name = profile.unwrap_or(DEFAULT_MANAGED_PROFILE);
        scopes.push(format!("profile:{task_name}/{profile_name}"));
    }
    scopes
}
