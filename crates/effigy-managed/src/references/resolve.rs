use std::borrow::Cow;
use std::path::{Path, PathBuf};

use effigy_core::shell::shell_quote;
use effigy_manifest::{
    resolve_task_execution_binding, LoadedCatalog, ManifestManagedRun, ManifestTaskRunIn,
    ResolvedTaskExecutionBinding, TaskResolverFn, TaskSelection,
};

use super::parser::{is_builtin_task_selector, ParsedTaskRef};
use super::ReferenceResolution;
use crate::profiles::has_concurrent_schema;
use crate::run_spec::{render_builtin_reference_invocation, render_task_run_spec, RunSpecContext};
use crate::{resolve_catalog_env_schema_with_ancestors, ManagedError};

pub enum ReferenceTarget<'a> {
    Builtin,
    Catalog(TaskSelection<'a>),
}

pub struct ResolvedReferenceRun {
    pub command: String,
    pub cwd: PathBuf,
}

pub fn resolve_reference_run<'request, 'a, F>(
    parsed: &ParsedTaskRef,
    resolution: ReferenceResolution<'request, 'a>,
    missing_run_error: F,
) -> Result<ResolvedReferenceRun, ManagedError>
where
    F: Fn(&TaskSelection<'a>) -> ManagedError,
{
    let ReferenceResolution {
        args_rendered,
        catalogs,
        task_scope_cwd,
        resolver,
        execution_root,
        ..
    } = resolution;
    let split_source_target = execution_root != task_scope_cwd;
    match resolve_reference_target(parsed, catalogs, task_scope_cwd, resolver) {
        Ok(ReferenceTarget::Builtin) => Ok(ResolvedReferenceRun {
            command: render_builtin_reference_invocation(
                &parsed.selector_rendered,
                args_rendered,
                if split_source_target {
                    execution_root
                } else {
                    task_scope_cwd
                },
            )?,
            cwd: if split_source_target {
                execution_root.to_path_buf()
            } else {
                task_scope_cwd.to_path_buf()
            },
        }),
        Ok(ReferenceTarget::Catalog(selection)) => {
            let cwd = if split_source_target {
                execution_root.to_path_buf()
            } else {
                selection.catalog.catalog_root.clone()
            };
            Ok(ResolvedReferenceRun {
                command: render_selected_task_invocation(
                    &selection,
                    &parsed.selector_rendered,
                    &parsed.selector.task_name,
                    resolution,
                    || missing_run_error(&selection),
                )?,
                cwd,
            })
        }
        Err(error) => Err(error),
    }
}

fn resolve_reference_target<'a>(
    parsed: &ParsedTaskRef,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &Path,
    resolver: TaskResolverFn<'_>,
) -> Result<ReferenceTarget<'a>, ManagedError> {
    match resolver(&parsed.selector, catalogs, task_scope_cwd) {
        Ok(selection) => Ok(ReferenceTarget::Catalog(selection)),
        Err(_) if is_builtin_task_selector(&parsed.selector) => Ok(ReferenceTarget::Builtin),
        Err(message) => Err(ManagedError::task_invocation(message)),
    }
}

fn render_selected_task_invocation<F>(
    selection: &TaskSelection<'_>,
    selector_rendered: &str,
    task_name: &str,
    resolution: ReferenceResolution<'_, '_>,
    missing_run_error: F,
) -> Result<String, ManagedError>
where
    F: FnOnce() -> ManagedError,
{
    let default_run_in = selection
        .catalog
        .manifest
        .task_defaults
        .as_ref()
        .and_then(|defaults| defaults.run_in);
    let effective_run_in = selection.task.effective_run_in(default_run_in);
    let execution_binding =
        resolve_task_execution_binding(&selection.catalog.manifest, task_name, selection.task)
            .map_err(|error| ManagedError::task_invocation(error.to_string()))?;
    let host_needs_container = effective_run_in == ManifestTaskRunIn::Container
        || matches!(
            execution_binding,
            Some(ResolvedTaskExecutionBinding::Workspace(_))
        );
    if let Some(run_spec) = selection.task.run.as_ref() {
        if resolution.host_launched && host_needs_container {
            return render_nested_catalog_invocation(selector_rendered, selection, resolution);
        }
        return render_selected_task_run(selection, run_spec, task_name, resolution);
    }

    if selection.task.mode.is_some()
        || has_concurrent_schema(selection.task)
        || execution_binding.is_some()
    {
        return render_nested_catalog_invocation(selector_rendered, selection, resolution);
    }

    Err(missing_run_error())
}

fn render_nested_catalog_invocation(
    selector_rendered: &str,
    selection: &TaskSelection<'_>,
    resolution: ReferenceResolution<'_, '_>,
) -> Result<String, ManagedError> {
    let args = nested_discovery_args(
        resolution.args_rendered,
        resolution.catalogs,
        &selection.catalog.catalog_root,
    );
    render_builtin_reference_invocation(
        selector_rendered,
        args.as_ref(),
        &selection.catalog.catalog_root,
    )
}

fn nested_discovery_args<'a>(
    args_rendered: &'a str,
    catalogs: &[LoadedCatalog],
    selected_root: &Path,
) -> Cow<'a, str> {
    let Some(origin) = catalogs
        .iter()
        .filter(|catalog| selected_root.starts_with(&catalog.catalog_root))
        .min_by_key(|catalog| catalog.depth)
        .map(|catalog| catalog.catalog_root.as_path())
    else {
        return Cow::Borrowed(args_rendered);
    };
    if origin == selected_root {
        return Cow::Borrowed(args_rendered);
    }
    let pin = format!("--repo {}", shell_quote(&origin.display().to_string()));
    if args_rendered.is_empty() {
        Cow::Owned(pin)
    } else {
        Cow::Owned(format!("{pin} {args_rendered}"))
    }
}

fn render_selected_task_run<'request, 'a>(
    selection: &TaskSelection<'a>,
    run_spec: &ManifestManagedRun,
    task_name: &str,
    resolution: ReferenceResolution<'request, 'a>,
) -> Result<String, ManagedError> {
    let ReferenceResolution {
        args_rendered,
        catalogs,
        task_scope_cwd,
        runtime_env_schema_override,
        depth,
        resolver,
        execution_root,
        invocation_cwd,
        ..
    } = resolution;
    let split_source_target = execution_root != task_scope_cwd;
    let repo_root = if split_source_target {
        execution_root
    } else {
        &selection.catalog.catalog_root
    };
    let nested_invocation_cwd = if split_source_target {
        invocation_cwd
    } else {
        &selection.catalog.catalog_root
    };
    let env_schema_resolved = resolve_catalog_env_schema_with_ancestors(
        catalogs,
        &selection.catalog.catalog_root,
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
            repo_root,
            bundle_root: selection.catalog.bundle_root.as_deref(),
            catalogs,
            task_scope_cwd: &selection.catalog.catalog_root,
            invocation_cwd: nested_invocation_cwd,
            runtime_env_schema_override,
            depth,
            resolver,
        },
    )
}
