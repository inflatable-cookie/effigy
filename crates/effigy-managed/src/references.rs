use std::path::{Path, PathBuf};

#[path = "references/context.rs"]
mod context;
#[path = "references/parser.rs"]
mod parser;
#[path = "references/resolve.rs"]
mod resolve;

use effigy_core::shell::shell_quote;
use effigy_manifest::TaskResolverFn;

use super::run_spec::wrap_reference_command_in_cwd;
use crate::ManagedError;
use context::{ManagedRefContext, StepRefContext};
use effigy_manifest::{LoadedCatalog, TaskSelection};
pub(crate) use parser::{merge_args_rendered, parse_task_ref};
use resolve::{resolve_reference_run, ResolvedReferenceRun};

#[derive(Clone, Copy)]
pub(crate) struct ReferenceResolution<'request, 'catalog> {
    pub(crate) args_rendered: &'request str,
    pub(crate) catalogs: &'catalog [LoadedCatalog],
    pub(crate) task_scope_cwd: &'request Path,
    pub(crate) execution_root: &'request Path,
    pub(crate) invocation_cwd: &'request Path,
    pub(crate) runtime_env_schema_override: Option<&'request Path>,
    pub(crate) depth: usize,
    pub(crate) resolver: TaskResolverFn<'catalog>,
    /// Declared backing-service aliases the host execution environment
    /// resolves to this checkout's effective aliases (effigy#431).
    pub(crate) host_env_aliases: &'catalog [crate::run_spec::HostAlias],
    /// True when the rendered command will run on the host (suite/sequence
    /// expansion). False when a managed parent will already exec it in a
    /// selected container.
    pub(crate) host_launched: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_task_reference_run<'a>(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    host_env_aliases: &'a [crate::run_spec::HostAlias],
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
            execution_root: task_scope_cwd,
            invocation_cwd: task_scope_cwd,
            runtime_env_schema_override: None,
            depth: 0,
            resolver,
            host_env_aliases,
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
    let parsed = parse_task_ref(&expand_reference_placeholders(
        task_ref,
        resolution.execution_root,
        resolution.task_scope_cwd,
    ))
    .map_err(|error| invalid_error(error.to_string()))?;
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

/// Expand the path placeholders a manifest run command already supports inside
/// a `{ task = "…" }` reference.
///
/// `{repo}` is the task's execution root (the selected catalog root) and
/// `{cwd}` is the invoking task's cwd, so a reference has a spelling that works
/// from a nested catalog (`{ task = "docs check links {repo}/README.md" }`).
/// Paths are shell-quoted because the reference's arguments are parsed as
/// shell words.
pub fn expand_reference_placeholders(
    task_ref: &str,
    execution_root: &Path,
    task_scope_cwd: &Path,
) -> String {
    let repo_rendered = shell_quote(&execution_root.display().to_string());
    let cwd_rendered = shell_quote(&task_scope_cwd.display().to_string());
    task_ref
        .replace("{repo}", &repo_rendered)
        .replace("{cwd}", &cwd_rendered)
}
