use std::path::{Path, PathBuf};

use super::super::super::catalog::select_catalog_and_task;
use super::super::super::managed::profiles::has_concurrent_schema;
use super::super::super::manifest::task_runtime::ManifestManagedRun;
use super::super::super::model::catalog::{LoadedCatalog, TaskSelection};
use super::super::super::{env_schema_support, error::RunnerError};
use super::super::run_spec::{
    render_builtin_reference_invocation, render_task_run_spec, RunSpecContext,
};
use super::parser::{is_builtin_task_selector, ParsedTaskRef};

pub(super) enum ReferenceTarget<'a> {
    Builtin,
    Catalog(TaskSelection<'a>),
}

pub(super) struct ResolvedReferenceRun {
    pub(super) command: String,
    pub(super) cwd: PathBuf,
}

pub(super) fn resolve_reference_run<'a, F>(
    parsed: &ParsedTaskRef,
    args_rendered: &str,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    runtime_env_schema_override: Option<&Path>,
    depth: usize,
    missing_run_error: F,
) -> Result<ResolvedReferenceRun, RunnerError>
where
    F: Fn(&TaskSelection<'a>) -> RunnerError,
{
    match resolve_reference_target(parsed, catalogs, task_scope_cwd) {
        Ok(ReferenceTarget::Builtin) => Ok(ResolvedReferenceRun {
            command: render_builtin_reference_invocation(&parsed.selector_rendered, args_rendered)?,
            cwd: task_scope_cwd.to_path_buf(),
        }),
        Ok(ReferenceTarget::Catalog(selection)) => Ok(ResolvedReferenceRun {
            command: render_selected_task_invocation(
                &selection,
                &parsed.selector_rendered,
                &parsed.selector.task_name,
                args_rendered,
                catalogs,
                runtime_env_schema_override,
                depth,
                || missing_run_error(&selection),
            )?,
            cwd: selection.catalog.catalog_root.clone(),
        }),
        Err(error) => Err(error),
    }
}

fn resolve_reference_target<'a>(
    parsed: &ParsedTaskRef,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<ReferenceTarget<'a>, RunnerError> {
    match select_catalog_and_task(&parsed.selector, catalogs, task_scope_cwd) {
        Ok(selection) => Ok(ReferenceTarget::Catalog(selection)),
        Err(_) if is_builtin_task_selector(&parsed.selector) => Ok(ReferenceTarget::Builtin),
        Err(error) => Err(error),
    }
}

fn render_selected_task_invocation<F>(
    selection: &TaskSelection<'_>,
    selector_rendered: &str,
    task_name: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&Path>,
    depth: usize,
    missing_run_error: F,
) -> Result<String, RunnerError>
where
    F: FnOnce() -> RunnerError,
{
    if let Some(run_spec) = selection.task.run.as_ref() {
        return render_selected_task_run(
            selection,
            run_spec,
            task_name,
            args_rendered,
            catalogs,
            runtime_env_schema_override,
            depth,
        );
    }

    if selection.task.mode.is_some()
        || has_concurrent_schema(selection.task)
        || selection.task.container_session.is_some()
    {
        return render_builtin_reference_invocation(selector_rendered, args_rendered);
    }

    Err(missing_run_error())
}

fn render_selected_task_run(
    selection: &TaskSelection<'_>,
    run_spec: &ManifestManagedRun,
    task_name: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    runtime_env_schema_override: Option<&Path>,
    depth: usize,
) -> Result<String, RunnerError> {
    let env_schema_resolved = env_schema_support::resolve_catalog_env_schema(
        &selection.catalog.catalog_root,
        selection.catalog.manifest.env_schema.as_ref(),
        runtime_env_schema_override,
    )?;
    let mut task_env = env_schema_resolved
        .as_ref()
        .map(|resolved| resolved.plain_env())
        .unwrap_or_default();
    for (key, value) in &selection.task.env {
        task_env.insert(key.clone(), value.clone());
    }

    render_task_run_spec(
        run_spec,
        RunSpecContext {
            task_name,
            task_env: &task_env,
            task_env_file: selection.task.env_file.as_ref(),
            env_profiles: &selection.catalog.manifest.env,
            args_rendered,
            args_raw: &[],
            repo_root: &selection.catalog.catalog_root,
            catalogs,
            task_scope_cwd: &selection.catalog.catalog_root,
            runtime_env_schema_override,
            depth,
        },
    )
}
