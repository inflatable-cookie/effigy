use std::path::Path;

use effigy_manifest::{LoadedCatalog, ManifestTask, TaskResolverFn};
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

use crate::{plan, profiles, ManagedError, ManagedTaskPlan};

/// Resolve a managed session's process plan.
///
/// `host_env_aliases` are the declared backing-service aliases the host
/// execution environment resolves to this checkout's effective aliases
/// (effigy#431); an empty slice renders every env value as written.
#[allow(clippy::too_many_arguments)]
pub fn resolve_managed_task_plan<'a>(
    selector: &TaskSelector,
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
    runtime_args: &'a TaskRuntimeArgs,
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &'a Path,
    host_env_aliases: &'a [crate::run_spec::HostAlias],
    resolver: TaskResolverFn<'a>,
) -> Result<Option<ManagedTaskPlan>, ManagedError> {
    let Some(mode) = task.mode.as_deref() else {
        // A task that declares `concurrent = [...]` (or per-profile
        // concurrent entries) but no `mode` would otherwise be
        // silently downgraded to standard execution, dropping every
        // concurrent process. That's almost never what the author
        // wanted — turn it into a hard error so the manifest mistake
        // surfaces at task-resolution time.
        if profiles::has_concurrent_schema(task) {
            return Err(ManagedError::TaskHasConcurrentWithoutMode {
                task: selector.task_name.clone(),
            });
        }
        return Ok(None);
    };
    if mode != "tui" {
        return Err(ManagedError::TaskManagedUnsupportedMode {
            task: selector.task_name.clone(),
            mode: mode.to_owned(),
        });
    }

    let profile = match profiles::select_concurrent_profile(selector, task, runtime_args) {
        Ok(profile) => profile,
        Err(error) => {
            // A plain `[tasks.dev]` bound to an environment is a managed
            // session over that environment: no `concurrent` entry per
            // process is needed (effigy#322).
            match profiles::environment_only_profile(task, runtime_args) {
                Some(profile) if plan::task_is_environment_bound(catalog, selector, task)? => {
                    profile
                }
                _ => return Err(error),
            }
        }
    };
    plan::resolve_managed_concurrent_task_plan(plan::ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile,
        passthrough: &runtime_args.passthrough,
        catalogs,
        task_scope_cwd,
        host_env_aliases,
        resolver,
    })
    .map(Some)
}
