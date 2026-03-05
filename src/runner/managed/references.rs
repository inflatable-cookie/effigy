use std::path::{Path, PathBuf};

#[path = "references/context.rs"]
mod context;
#[path = "references/invocation.rs"]
mod invocation;
#[path = "references/parser.rs"]
mod parser;

use super::super::catalog::select_catalog_and_task;
use super::super::util::shell_quote;
use super::super::{LoadedCatalog, ManifestManagedRun, RunnerError, TaskSelection};
use super::run_spec::{render_task_run_spec, RunSpecContext};
use context::{ManagedRefContext, StepRefContext};
use invocation::render_builtin_task_reference_invocation;
use parser::{is_builtin_task_selector, merge_args_rendered, parse_task_ref, ParsedTaskRef};

enum ReferenceTarget<'a> {
    Builtin,
    Catalog(TaskSelection<'a>),
}

pub(super) fn resolve_task_reference_run(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    let context = ManagedRefContext {
        managed_task_name: managed_task_name.to_owned(),
        process_name: process_name.to_owned(),
        task_ref: task_ref.to_owned(),
    };
    let parsed = parse_task_ref(task_ref).map_err(|error| context.invalid(error))?;
    match resolve_reference_target(&parsed, catalogs, task_scope_cwd) {
        Ok(ReferenceTarget::Builtin) => {
            let command = render_builtin_task_reference_invocation(
                &parsed.selector_rendered,
                &parsed.args_rendered,
            )?;
            Ok((command, task_scope_cwd.to_path_buf()))
        }
        Ok(ReferenceTarget::Catalog(selection)) => {
            let run_spec = selection.task.run.as_ref().ok_or_else(|| {
                context.invalid(format!(
                    "referenced task `{}` in {} has no `run` command",
                    parsed.selector.task_name,
                    selection.catalog.manifest_path.display()
                ))
            })?;
            let run_rendered = render_selected_task_run(
                &selection,
                run_spec,
                &parsed.selector.task_name,
                &parsed.args_rendered,
                catalogs,
                0,
            )
            .map_err(|error| context.invalid(error))?;
            Ok((run_rendered, selection.catalog.catalog_root.clone()))
        }
        Err(error) => Err(context.invalid(error)),
    }
}

pub(super) fn resolve_task_reference_step(
    task_name: &str,
    task_ref: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    let context = StepRefContext {
        task_name: task_name.to_owned(),
        task_ref: task_ref.to_owned(),
    };
    let parsed = parse_task_ref(task_ref).map_err(|error| context.invalid(error))?;
    let merged_args_rendered = merge_args_rendered(&parsed.args_rendered, args_rendered);
    match resolve_reference_target(&parsed, catalogs, task_scope_cwd) {
        Ok(ReferenceTarget::Builtin) => {
            let command = render_builtin_task_reference_invocation(
                &parsed.selector_rendered,
                &merged_args_rendered,
            )
            .map_err(|detail| context.failure(detail))?;
            Ok(render_cwd_wrapped_command(task_scope_cwd, &command))
        }
        Ok(ReferenceTarget::Catalog(selection)) => {
            let run_spec = selection.task.run.as_ref().ok_or_else(|| {
                context.failure(format!(
                    "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
                    selection.catalog.manifest_path.display()
                ))
            })?;
            let nested = render_selected_task_run(
                &selection,
                run_spec,
                &parsed.selector.task_name,
                &merged_args_rendered,
                catalogs,
                depth,
            )?;
            Ok(render_cwd_wrapped_command(
                &selection.catalog.catalog_root,
                &nested,
            ))
        }
        Err(error) => Err(context.failure(error)),
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

fn render_cwd_wrapped_command(cwd: &Path, command: &str) -> String {
    format!(
        "(cd {} && {})",
        shell_quote(&cwd.display().to_string()),
        command
    )
}
