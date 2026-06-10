use std::path::{Path, PathBuf};

#[path = "references/context.rs"]
mod context;
#[path = "references/parser.rs"]
mod parser;
#[path = "references/resolve.rs"]
mod resolve;

use effigy_manifest::TaskResolverFn;

use super::run_spec::wrap_reference_command_in_cwd;
use crate::ManagedError;
use context::{ManagedRefContext, StepRefContext};
use effigy_manifest::{LoadedCatalog, TaskSelection};
use parser::{merge_args_rendered, parse_task_ref};
use resolve::{resolve_reference_run, ResolvedReferenceRun};

pub fn resolve_task_reference_run<'a>(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    resolver: TaskResolverFn<'a>,
) -> Result<(String, PathBuf), ManagedError> {
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
        None,
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
        resolver,
    )?;
    Ok((resolved.command, resolved.cwd))
}

pub fn resolve_task_reference_step<'a>(
    task_name: &str,
    task_ref: &str,
    args_rendered: &str,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    runtime_env_schema_override: Option<&Path>,
    depth: usize,
    resolver: TaskResolverFn<'a>,
) -> Result<String, ManagedError> {
    let context = StepRefContext {
        task_name: task_name.to_owned(),
        task_ref: task_ref.to_owned(),
    };
    let resolved = resolve_task_reference(
        task_ref,
        args_rendered,
        catalogs,
        task_scope_cwd,
        runtime_env_schema_override,
        depth,
        |detail| context.invalid(detail),
        |detail| context.failure(detail),
        |_, selection| {
            context.failure(format!(
                "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
                selection.catalog.manifest_path.display()
            ))
        },
        resolver,
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
    runtime_env_schema_override: Option<&Path>,
    depth: usize,
    invalid_error: FInvalid,
    resolve_error: FResolve,
    missing_run_error: FMissing,
    resolver: TaskResolverFn<'a>,
) -> Result<ResolvedReferenceRun, ManagedError>
where
    FInvalid: Fn(String) -> ManagedError,
    FResolve: Fn(String) -> ManagedError,
    FMissing: Fn(&parser::ParsedTaskRef, &TaskSelection<'a>) -> ManagedError,
{
    let parsed = parse_task_ref(task_ref).map_err(|error| invalid_error(error.to_string()))?;
    let merged_args_rendered = merge_args_rendered(&parsed.args_rendered, args_rendered);
    resolve_reference_run(
        &parsed,
        &merged_args_rendered,
        catalogs,
        task_scope_cwd,
        runtime_env_schema_override,
        depth,
        |selection| missing_run_error(&parsed, selection),
        resolver,
    )
    .map_err(|error| resolve_error(error.to_string()))
}
