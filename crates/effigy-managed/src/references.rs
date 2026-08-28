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

#[derive(Clone, Copy)]
pub(crate) struct ReferenceResolution<'request, 'catalog> {
    pub(crate) args_rendered: &'request str,
    pub(crate) catalogs: &'catalog [LoadedCatalog],
    pub(crate) task_scope_cwd: &'request Path,
    pub(crate) runtime_env_schema_override: Option<&'request Path>,
    pub(crate) depth: usize,
    pub(crate) resolver: TaskResolverFn<'catalog>,
    /// True when the rendered command will run on the host (suite/sequence
    /// expansion). False when a managed parent will already exec it in a
    /// selected container.
    pub(crate) host_launched: bool,
}

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
        ReferenceResolution {
            args_rendered: "",
            catalogs,
            task_scope_cwd,
            runtime_env_schema_override: None,
            depth: 0,
            resolver,
            host_launched: false,
        },
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

pub(crate) fn resolve_task_reference_step<'request, 'catalog>(
    task_name: &str,
    task_ref: &str,
    resolution: ReferenceResolution<'request, 'catalog>,
) -> Result<String, ManagedError> {
    let context = StepRefContext {
        task_name: task_name.to_owned(),
        task_ref: task_ref.to_owned(),
    };
    let resolved = resolve_task_reference(
        task_ref,
        resolution,
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
    resolution: ReferenceResolution<'_, 'a>,
    invalid_error: FInvalid,
    resolve_error: FResolve,
    missing_run_error: FMissing,
) -> Result<ResolvedReferenceRun, ManagedError>
where
    FInvalid: Fn(String) -> ManagedError,
    FResolve: Fn(String) -> ManagedError,
    FMissing: Fn(&parser::ParsedTaskRef, &TaskSelection<'a>) -> ManagedError,
{
    let parsed = parse_task_ref(task_ref).map_err(|error| invalid_error(error.to_string()))?;
    let merged_args_rendered = merge_args_rendered(&parsed.args_rendered, resolution.args_rendered);
    resolve_reference_run(
        &parsed,
        ReferenceResolution {
            args_rendered: &merged_args_rendered,
            ..resolution
        },
        |selection| missing_run_error(&parsed, selection),
    )
    .map_err(|error| resolve_error(error.to_string()))
}
