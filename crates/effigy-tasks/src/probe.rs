use std::collections::BTreeSet;
use std::path::Path;

use effigy_core::builtin_tasks::is_builtin_task_name;
use effigy_manifest::{LoadedCatalog, ManifestTask, TaskSelection};
use effigy_routing::select_catalog_and_task;
use serde_json::json;

use crate::EffigyTasksError;
use crate::{parse_task_reference_invocation, render_task_selector, TaskSelector};

const DEFAULT_MANAGED_PROFILE: &str = "default";
pub struct ProbeTaskResolutionRequest<'a> {
    pub raw_selector: Option<&'a str>,
    pub cwd: &'a Path,
    pub deferred_builtins: &'a BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct TaskResolutionProbe {
    selector: String,
    status: &'static str,
    catalog: Option<String>,
    catalog_root: Option<String>,
    task: String,
    lock_scopes: Vec<String>,
    evidence: Vec<String>,
    error: Option<String>,
}

impl TaskResolutionProbe {
    fn ok(
        selector: &str,
        task: &str,
        catalog: Option<String>,
        catalog_root: Option<String>,
        lock_scopes: Vec<String>,
        evidence: Vec<String>,
    ) -> Self {
        Self {
            selector: selector.to_owned(),
            status: "ok",
            catalog,
            catalog_root,
            task: task.to_owned(),
            lock_scopes,
            evidence,
            error: None,
        }
    }

    fn error(selector: &str, task: &str, lock_scopes: Vec<String>, error: String) -> Self {
        Self {
            selector: selector.to_owned(),
            status: "error",
            catalog: None,
            catalog_root: None,
            task: task.to_owned(),
            lock_scopes,
            evidence: Vec::new(),
            error: Some(error),
        }
    }

    pub fn into_json(self) -> serde_json::Value {
        json!({
            "selector": self.selector,
            "status": self.status,
            "catalog": self.catalog,
            "catalog_root": self.catalog_root,
            "task": self.task,
            "lock_scopes": self.lock_scopes,
            "evidence": self.evidence,
            "error": self.error,
        })
    }
}

pub fn probe_task_resolution(
    request: ProbeTaskResolutionRequest<'_>,
    catalogs: &[LoadedCatalog],
) -> Result<Option<TaskResolutionProbe>, EffigyTasksError> {
    let Some(raw_selector) = request.raw_selector else {
        return Ok(None);
    };
    let (selector, selector_args) =
        parse_task_reference_invocation(raw_selector).map_err(EffigyTasksError::message)?;
    let selector_task_name = selector.task_name.clone();

    let probe = match select_catalog_and_task(&selector, catalogs, request.cwd) {
        Ok(selection) => build_selected_probe(
            raw_selector,
            &selector,
            &selector_task_name,
            &selector_args,
            selection,
        ),
        Err(_error)
            if is_builtin_task_name(&selector_task_name)
                && !request.deferred_builtins.contains(&selector_task_name) =>
        {
            TaskResolutionProbe::ok(
                raw_selector,
                &selector_task_name,
                None,
                None,
                Vec::new(),
                vec![format!("resolved built-in task `{selector_task_name}`")],
            )
        }
        Err(error) => TaskResolutionProbe::error(
            raw_selector,
            &selector_task_name,
            Vec::new(),
            error.to_string(),
        ),
    };
    Ok(Some(probe))
}

fn build_selected_probe(
    raw_selector: &str,
    selector: &TaskSelector,
    selector_task_name: &str,
    selector_args: &[String],
    selection: TaskSelection<'_>,
) -> TaskResolutionProbe {
    if selection.task.mode.as_deref() != Some("tui") {
        return TaskResolutionProbe::ok(
            raw_selector,
            selector_task_name,
            Some(selection.catalog.alias.clone()),
            Some(selection.catalog.catalog_root.display().to_string()),
            lock_scopes_for_task(selector, selection.task, None),
            selection.evidence,
        );
    }

    let profile_name = selector_args
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_MANAGED_PROFILE.to_owned());
    let lock_scopes = lock_scopes_for_task(selector, selection.task, Some(&profile_name));
    if !has_concurrent_profile(selection.task, &profile_name) {
        let available_display = render_available_profiles(selection.task);
        return TaskResolutionProbe::error(
            raw_selector,
            selector_task_name,
            lock_scopes,
            format!(
                "managed profile `{profile_name}` not found for task `{selector_task_name}`; available: {available_display}"
            ),
        );
    }

    let mut evidence = selection.evidence;
    evidence.push(format!(
        "managed profile `{profile_name}` resolved via invocation `{raw_selector}`"
    ));
    TaskResolutionProbe::ok(
        raw_selector,
        selector_task_name,
        Some(selection.catalog.alias.clone()),
        Some(selection.catalog.catalog_root.display().to_string()),
        lock_scopes,
        evidence,
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

fn selector_lock_name(selector: &TaskSelector) -> String {
    render_task_selector(selector)
}

fn task_lock_scope_label(task: &ManifestTask, selector: &TaskSelector) -> String {
    match task.lock.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => format!("shared:{name}"),
        _ => format!("task:{}", selector_lock_name(selector)),
    }
}

fn lock_scopes_for_task(
    selector: &TaskSelector,
    task: &ManifestTask,
    profile: Option<&str>,
) -> Vec<String> {
    let task_name = selector_lock_name(selector);
    let mut scopes = vec![task_lock_scope_label(task, selector)];
    if task.mode.as_deref() == Some("tui") {
        let profile_name = profile.unwrap_or(DEFAULT_MANAGED_PROFILE);
        scopes.push(format!("profile:{task_name}/{profile_name}"));
    }
    scopes
}

fn has_concurrent_schema(task: &ManifestTask) -> bool {
    !task.concurrent.is_empty()
        || task
            .profiles
            .values()
            .any(|profile| profile.concurrent_entries().is_some())
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
    if !task.concurrent.is_empty() && !available.iter().any(|name| name == DEFAULT_MANAGED_PROFILE)
    {
        available.push(DEFAULT_MANAGED_PROFILE.to_owned());
    }
    available.sort();
    available
}

fn has_concurrent_profile(task: &ManifestTask, profile_name: &str) -> bool {
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
