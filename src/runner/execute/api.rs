use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_context::EffigyRuntimeContext;
use effigy_execution::{
    ExecutionEnvironmentPlan, ExecutionSurface, TaskExecutionRequest, TaskExecutionRequestBuilder,
};
use effigy_manifest::{LoadedCatalog, ManifestTask, TaskSelection};
use effigy_tasks::CatalogSelectionMode;

use super::planning::{
    build_execution_preflight as build_execution_preflight_impl, ExecutionPreflight,
};
use crate::runner::error::RunnerError;

pub(in crate::runner) fn run_manifest_task_request(
    request: TaskExecutionRequest,
) -> Result<String, RunnerError> {
    super::entry::run_manifest_task_request(request)
}

#[cfg(test)]
pub(in crate::runner) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    run_manifest_task_with_surface(task, cwd, ExecutionSurface::DirectCli)
}

pub(in crate::runner) fn run_manifest_task_with_surface(
    task: &TaskInvocation,
    cwd: PathBuf,
    surface: ExecutionSurface,
) -> Result<String, RunnerError> {
    run_manifest_task_with_surface_and_env(task, cwd, surface, &BTreeMap::new())
}

pub(in crate::runner) fn run_manifest_task_with_surface_and_env(
    task: &TaskInvocation,
    cwd: PathBuf,
    surface: ExecutionSurface,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    run_manifest_task_with_surface_env_and_secret_targets(task, cwd, surface, env_overrides, &[])
}

pub(in crate::runner) fn run_manifest_task_with_surface_env_and_secret_targets(
    task: &TaskInvocation,
    cwd: PathBuf,
    surface: ExecutionSurface,
    env_overrides: &BTreeMap<String, String>,
    secret_targets: &[&str],
) -> Result<String, RunnerError> {
    let runtime_context = crate::runner::command_context::active_runtime_context()
        .filter(|context| context.task_source().is_some())
        .unwrap_or(
            EffigyRuntimeContext::capture_lossy(Some(cwd.clone()), None)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?,
        );
    let mut environment = ExecutionEnvironmentPlan::default().cwd(cwd);
    for (key, value) in env_overrides {
        environment = environment.env(key.clone(), value.clone());
    }
    for target in secret_targets {
        environment = environment.secret_target((*target).to_owned());
    }
    let request = TaskExecutionRequestBuilder::new()
        .runtime_context(runtime_context)
        .task(task.name.clone(), task.args.clone())
        .surface(surface)
        .environment(environment)
        .build()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_manifest_task_request(request)
}

pub(in crate::runner) fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    build_execution_preflight_impl(task, cwd)
}

/// Catalog that owns an inline hook invoked at `cwd`. Nested builtins
/// (`secrets init`, `secrets set`) inherit `selection.catalog.catalog_root`
/// as `--repo`, so a generate hook declared on a child catalog must not bind
/// to the workspace `resolved_root`.
fn catalog_for_inline_cwd<'a>(
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
    resolved_root: &Path,
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .filter(|catalog| cwd.starts_with(&catalog.catalog_root))
        .max_by_key(|catalog| catalog.catalog_root.as_os_str().len())
        .or_else(|| {
            catalogs
                .iter()
                .filter(|catalog| catalog.catalog_root == resolved_root)
                .min_by_key(|catalog| catalog.depth)
        })
}

pub(in crate::runner) fn run_inline_task_with_cwd_and_env(
    mut task: ManifestTask,
    cwd: PathBuf,
    label: &str,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let invocation = TaskInvocation {
        name: label.to_owned(),
        args: Vec::new(),
    };
    let preflight = super::planning::build_execution_preflight(&invocation, cwd)?;
    let inline_catalog = catalog_for_inline_cwd(
        &preflight.catalogs,
        &preflight.invocation_cwd,
        &preflight.resolved.resolved_root,
    )
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "bootstrap run could not resolve catalog for {}",
            preflight.invocation_cwd.display()
        ))
    })?;

    for (key, value) in env_overrides {
        task.env.insert(key.clone(), value.clone());
    }
    let selection = TaskSelection {
        catalog: inline_catalog,
        task: &task,
        mode: CatalogSelectionMode::CwdNearest,
        evidence: vec!["inline task".to_owned()],
        surface: effigy_tasks::TaskSurface::Published,
    };
    let selection_plan = super::selection::build_execution_selection_plan(&preflight, &selection);
    super::pipeline::standard::run_standard_task(
        &preflight,
        &selection,
        &selection_plan,
        None,
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use effigy_manifest::TaskManifest;

    use super::*;

    fn manifest_from_toml(body: &str) -> TaskManifest {
        toml::from_str(body).expect("parse manifest")
    }

    fn loaded_catalog(
        alias: &str,
        root: &str,
        manifest: TaskManifest,
        depth: usize,
    ) -> LoadedCatalog {
        LoadedCatalog::for_test(alias, PathBuf::from(root), manifest, depth)
    }

    #[test]
    fn catalog_for_inline_cwd_selects_the_child_catalog_not_the_workspace() {
        let root = loaded_catalog("root", "/workspace", manifest_from_toml(""), 0);
        let child = loaded_catalog(
            "desktop",
            "/workspace/apps/bovine-desktop",
            manifest_from_toml(""),
            1,
        );
        let catalogs = vec![root, child];
        let selected = catalog_for_inline_cwd(
            &catalogs,
            Path::new("/workspace/apps/bovine-desktop"),
            Path::new("/workspace"),
        )
        .expect("child catalog");
        assert_eq!(selected.alias, "desktop");
        let workspace =
            catalog_for_inline_cwd(&catalogs, Path::new("/workspace"), Path::new("/workspace"))
                .expect("workspace catalog");
        assert_eq!(workspace.alias, "root");
    }
}
