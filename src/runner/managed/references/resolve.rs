use std::path::{Path, PathBuf};

use super::super::super::catalog::select_catalog_and_task;
use super::super::super::manifest::task_runtime::ManifestManagedRun;
use super::super::super::model::catalog::{LoadedCatalog, TaskSelection};
use super::super::run_spec::{
    render_builtin_reference_invocation, render_task_run_spec, RunSpecContext,
};
use super::parser::{is_builtin_task_selector, ParsedTaskRef};
use crate::runner::error::RunnerError;

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
        Ok(ReferenceTarget::Catalog(selection)) => {
            let run_spec = selection
                .task
                .run
                .as_ref()
                .ok_or_else(|| missing_run_error(&selection))?;
            Ok(ResolvedReferenceRun {
                command: render_selected_task_run(
                    &selection,
                    run_spec,
                    &parsed.selector.task_name,
                    args_rendered,
                    catalogs,
                    depth,
                )?,
                cwd: selection.catalog.catalog_root.clone(),
            })
        }
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

fn render_selected_task_run(
    selection: &TaskSelection<'_>,
    run_spec: &ManifestManagedRun,
    task_name: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    depth: usize,
) -> Result<String, RunnerError> {
    render_task_run_spec(
        run_spec,
        RunSpecContext {
            task_name,
            task_env: &selection.task.env,
            task_env_file: selection.task.env_file.as_ref(),
            env_profiles: &selection.catalog.manifest.env,
            args_rendered,
            repo_root: &selection.catalog.catalog_root,
            catalogs,
            task_scope_cwd: &selection.catalog.catalog_root,
            depth,
        },
    )
}
