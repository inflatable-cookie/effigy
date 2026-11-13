use std::borrow::Cow;
use std::path::{Path, PathBuf};

use effigy_core::shell::shell_quote;
use effigy_manifest::{
    resolve_task_execution_binding, LoadedCatalog, ManifestManagedRun, ManifestTaskRunIn,
    ResolvedTaskExecutionBinding, TaskResolveError, TaskResolverFn, TaskSelection,
};

use super::parser::{is_builtin_task_selector, ParsedTaskRef};
use super::ReferenceResolution;
use crate::profiles::has_concurrent_schema;
use crate::run_spec::{
    render_builtin_reference_invocation, resolve_task_run_spec, RenderedRun, RunSpecContext,
};
use crate::{resolve_catalog_env_schema_with_ancestors, ManagedError};

pub enum ReferenceTarget<'a> {
    Builtin,
    Catalog(TaskSelection<'a>),
}

pub struct ResolvedReferenceRun {
    pub run: RenderedRun,
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
            run: RenderedRun {
                command: render_builtin_reference_invocation(
                    &parsed.selector_rendered,
                    args_rendered,
                    if split_source_target {
                        execution_root
                    } else {
                        task_scope_cwd
                    },
                )?,
                env: Default::default(),
            },
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
                run: render_selected_task_invocation(
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
        Err(error) if may_fall_back_to_builtin(&error, &parsed.selector) => {
            Ok(ReferenceTarget::Builtin)
        }
        Err(error) => Err(ManagedError::task_invocation(error.to_string())),
    }
}

/// Whether a failed catalog resolution may fall back to the builtin of the
/// same name.
///
/// Only "no catalog declares this task" qualifies. A catalog graph that could
/// not be read, a manifest that did not parse, or an ambiguous selection must
/// refuse instead: treating those as "not a catalog task" would run the builtin
/// while the declared task existed but could not be seen.
fn may_fall_back_to_builtin(
    error: &TaskResolveError,
    selector: &effigy_tasks::TaskSelector,
) -> bool {
    error.is_not_found() && is_builtin_task_selector(selector)
}

fn render_selected_task_invocation<F>(
    selection: &TaskSelection<'_>,
    selector_rendered: &str,
    task_name: &str,
    resolution: ReferenceResolution<'_, '_>,
    missing_run_error: F,
) -> Result<RenderedRun, ManagedError>
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
        if resolution.host_launched
            && (host_needs_container || matches!(run_spec, ManifestManagedRun::Sequence(_)))
        {
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
) -> Result<RenderedRun, ManagedError> {
    let args = nested_discovery_args(
        resolution.args_rendered,
        resolution.catalogs,
        &selection.catalog.catalog_root,
    );
    Ok(RenderedRun {
        command: render_builtin_reference_invocation(
            selector_rendered,
            args.as_ref(),
            &selection.catalog.catalog_root,
        )?,
        env: Default::default(),
    })
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
) -> Result<RenderedRun, ManagedError> {
    let ReferenceResolution {
        args_rendered,
        catalogs,
        task_scope_cwd,
        runtime_env_schema_override,
        depth,
        resolver,
        execution_root,
        invocation_cwd,
        host_env_aliases,
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

    resolve_task_run_spec(
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
            host_env_aliases,
            depth,
            resolver,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use effigy_manifest::{LoadedCatalog, TaskResolveError, TaskResolverFn, TaskSelection};

    use super::{resolve_reference_target, ReferenceTarget};
    use crate::references::parser::parse_task_ref;

    fn resolver(
        error: TaskResolveError,
    ) -> impl for<'c> Fn(
        &effigy_tasks::TaskSelector,
        &'c [LoadedCatalog],
        &Path,
    ) -> Result<TaskSelection<'c>, TaskResolveError> {
        move |_, _, _| Err(error.clone())
    }

    /// A reference to a builtin name falls back only when no catalog declares
    /// it. Before the typed failure class this arm matched every resolver
    /// failure, so an unreadable catalog graph silently ran the builtin.
    #[test]
    fn a_builtin_reference_falls_back_only_when_no_catalog_declares_it() {
        let parsed = parse_task_ref("cache").expect("a builtin reference");
        let catalogs: Vec<LoadedCatalog> = Vec::new();

        let fallback: TaskResolverFn<'_> =
            &resolver(TaskResolveError::not_found("no catalog declares `cache`"));
        assert!(matches!(
            resolve_reference_target(&parsed, &catalogs, Path::new("/repo"), fallback),
            Ok(ReferenceTarget::Builtin)
        ));

        for error in [
            TaskResolveError::unavailable("failed to read /repo/effigy.toml"),
            TaskResolveError::ambiguous("`cache` is declared by `repo` and `services`"),
        ] {
            let failing: TaskResolverFn<'_> = &resolver(error);
            let outcome = resolve_reference_target(&parsed, &catalogs, Path::new("/repo"), failing);
            assert!(
                outcome.is_err(),
                "a catalog graph that could not answer must refuse, not run the builtin"
            );
        }
    }

    /// A task that is not a builtin name never falls back, whatever the class.
    #[test]
    fn a_task_reference_never_falls_back_to_a_builtin() {
        let parsed = parse_task_ref("build").expect("a task reference");
        let catalogs: Vec<LoadedCatalog> = Vec::new();
        let missing: TaskResolverFn<'_> =
            &resolver(TaskResolveError::not_found("no catalog declares `build`"));
        assert!(resolve_reference_target(&parsed, &catalogs, Path::new("/repo"), missing).is_err());
    }
}
