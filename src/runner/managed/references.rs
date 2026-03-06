use std::path::{Path, PathBuf};

#[path = "references/context.rs"]
mod context;
#[path = "references/parser.rs"]
mod parser;
#[path = "references/resolve.rs"]
mod resolve;

use super::super::model::catalog::{LoadedCatalog, TaskSelection};
use super::run_spec::wrap_reference_command_in_cwd;
use crate::runner::error::RunnerError;
use context::{ManagedRefContext, StepRefContext};
use parser::{merge_args_rendered, parse_task_ref};
use resolve::{resolve_reference_run, ResolvedReferenceRun};

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
    let resolved = resolve_task_reference(
        task_ref,
        "",
        catalogs,
        task_scope_cwd,
        0,
        |detail| context.invalid(detail),
        |detail| context.invalid(detail),
        |parsed, selection| {
            context.invalid(format!(
                "referenced task `{}` in {} has no `run` command",
                parsed.selector.task_name,
                selection.catalog.manifest_path.display()
            ))
        },
    )?;
    Ok((resolved.command, resolved.cwd))
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
    let resolved = resolve_task_reference(
        task_ref,
        args_rendered,
        catalogs,
        task_scope_cwd,
        depth,
        |detail| context.invalid(detail),
        |detail| context.failure(detail),
        |_, selection| {
            context.failure(format!(
                "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
                selection.catalog.manifest_path.display()
            ))
        },
    )?;
    Ok(wrap_reference_command_in_cwd(
        &resolved.cwd,
        &resolved.command,
    ))
}

fn resolve_task_reference<'a, FInvalid, FResolve, FMissing>(
    task_ref: &str,
    args_rendered: &str,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
    invalid_error: FInvalid,
    resolve_error: FResolve,
    missing_run_error: FMissing,
) -> Result<ResolvedReferenceRun, RunnerError>
where
    FInvalid: Fn(String) -> RunnerError,
    FResolve: Fn(String) -> RunnerError,
    FMissing: Fn(&parser::ParsedTaskRef, &TaskSelection<'a>) -> RunnerError,
{
    let parsed = parse_task_ref(task_ref).map_err(|error| invalid_error(error.to_string()))?;
    let merged_args_rendered = merge_args_rendered(&parsed.args_rendered, args_rendered);
    resolve_reference_run(
        &parsed,
        &merged_args_rendered,
        catalogs,
        task_scope_cwd,
        depth,
        |selection| missing_run_error(&parsed, selection),
    )
    .map_err(|error| resolve_error(error.to_string()))
}
