use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_context::EffigyRuntimeContext;
use effigy_execution::{
    ExecutionEnvironmentPlan, ExecutionSurface, TaskExecutionRequest, TaskExecutionRequestBuilder,
};
use effigy_manifest::{
    LoadedCatalog, ManifestContainersConfig, ManifestSystemsConfig, ManifestTask,
    ManifestTaskRunIn, TaskSelection,
};
use effigy_tasks::CatalogSelectionMode;

use super::planning::{
    build_execution_preflight as build_execution_preflight_impl, ExecutionPreflight,
};
use super::selection::{resolve_task_selection, SelectionResolution};
use crate::runner::error::RunnerError;

pub(in crate::runner) use super::binding::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution,
    ContainerExecutionBinding, ExecutionBindingKind, ExecutionBindingResolution,
    InlineWorkspaceCapabilitySurface,
};

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

pub(in crate::runner) fn task_requires_container_runtime(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<bool, RunnerError> {
    let preflight = build_execution_preflight_impl(task, cwd)?;
    let selection = match resolve_task_selection(task, &preflight)? {
        SelectionResolution::Selected { selection, .. } => selection,
        SelectionResolution::Output(_) => return Ok(false),
    };
    let (default_run_in, systems, containers) =
        effective_task_binding_inputs(&preflight.invocation_cwd, &preflight.catalogs, &selection);

    let binding_resolution = resolve_execution_binding_resolution(
        default_run_in,
        systems.as_ref(),
        containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "bootstrap backend selection",
    )?;
    Ok(binding_resolution.is_inline_container()
        || matches!(
            binding_resolution.kind(),
            ExecutionBindingKind::NamedContainer
        ))
}

pub(in crate::runner) fn effective_task_binding_inputs<'a>(
    invocation_cwd: &Path,
    catalogs: &'a [LoadedCatalog],
    selection: &TaskSelection<'a>,
) -> (
    Option<ManifestTaskRunIn>,
    Option<ManifestSystemsConfig>,
    Option<ManifestContainersConfig>,
) {
    let scope_catalog = scope_root_catalog_from_catalogs(invocation_cwd, catalogs);
    let default_run_in = selection
        .catalog
        .manifest
        .task_defaults
        .as_ref()
        .and_then(|defaults| defaults.run_in)
        .or_else(|| {
            scope_catalog.and_then(|catalog| {
                catalog
                    .manifest
                    .task_defaults
                    .as_ref()
                    .and_then(|defaults| defaults.run_in)
            })
        });
    let systems = merge_systems_config(
        scope_catalog.and_then(|catalog| catalog.manifest.systems.as_ref()),
        selection.catalog.manifest.systems.as_ref(),
    );
    let containers = merge_containers_config(
        scope_containers_config_from_catalogs(invocation_cwd, catalogs),
        selection.catalog.manifest.containers.as_ref(),
    );
    (default_run_in, systems, containers)
}

pub(in crate::runner) fn execution_scope_root<'a>(
    invocation_cwd: &Path,
    catalogs: &'a [LoadedCatalog],
    selection: &TaskSelection<'a>,
) -> &'a Path {
    scope_root_catalog_from_catalogs(invocation_cwd, catalogs)
        .map(|catalog| catalog.catalog_root.as_path())
        .unwrap_or(selection.catalog.catalog_root.as_path())
}

fn scope_root_catalog_from_catalogs<'a>(
    invocation_cwd: &Path,
    catalogs: &'a [LoadedCatalog],
) -> Option<&'a LoadedCatalog> {
    catalogs
        .iter()
        .filter(|catalog| invocation_cwd.starts_with(&catalog.catalog_root))
        .max_by_key(|catalog| catalog.depth)
}

fn scope_containers_config_from_catalogs<'a>(
    invocation_cwd: &Path,
    catalogs: &'a [LoadedCatalog],
) -> Option<&'a ManifestContainersConfig> {
    catalogs
        .iter()
        .filter(|catalog| {
            invocation_cwd.starts_with(&catalog.catalog_root)
                && catalog.manifest.containers.is_some()
        })
        .max_by_key(|catalog| catalog.depth)
        .and_then(|catalog| catalog.manifest.containers.as_ref())
}

fn merge_systems_config(
    scope: Option<&ManifestSystemsConfig>,
    selected: Option<&ManifestSystemsConfig>,
) -> Option<ManifestSystemsConfig> {
    let mut merged = scope.cloned().unwrap_or_default();
    if let Some(selected) = selected {
        if selected.default.is_some() {
            merged.default.clone_from(&selected.default);
        }
        for (name, config) in &selected.systems {
            merged.systems.insert(name.clone(), config.clone());
        }
    }
    if merged.default.is_none() && merged.systems.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn merge_containers_config(
    scope: Option<&ManifestContainersConfig>,
    selected: Option<&ManifestContainersConfig>,
) -> Option<ManifestContainersConfig> {
    let mut merged = scope.cloned().unwrap_or_default();
    if let Some(selected) = selected {
        if selected.default.is_some() {
            merged.default.clone_from(&selected.default);
        }
        for (name, config) in &selected.environments {
            merged.environments.insert(name.clone(), config.clone());
        }
    }
    if merged.default.is_none() && merged.environments.is_empty() {
        None
    } else {
        Some(merged)
    }
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
    let root_catalog = preflight
        .catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == preflight.resolved.resolved_root)
        .min_by_key(|catalog| catalog.depth)
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "bootstrap run could not resolve root catalog for {}",
                preflight.resolved.resolved_root.display()
            ))
        })?;

    for (key, value) in env_overrides {
        task.env.insert(key.clone(), value.clone());
    }
    let selection = TaskSelection {
        catalog: root_catalog,
        task: &task,
        mode: CatalogSelectionMode::RootShallowest,
        evidence: vec!["inline task".to_owned()],
    };
    let selection_plan = super::selection::build_execution_selection_plan(&preflight, &selection);
    super::pipeline::standard::run_standard_task(&preflight, &selection, &selection_plan)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use effigy_manifest::TaskManifest;
    use effigy_tasks::CatalogSelectionMode;

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
        let root = PathBuf::from(root);
        LoadedCatalog {
            alias: alias.to_owned(),
            manifest_path: root.join("effigy.toml"),
            catalog_root: root,
            bundle_root: None,
            manifest,
            defer_run: None,
            deferred_builtins: BTreeSet::new(),
            depth,
        }
    }

    #[test]
    fn effective_task_binding_inputs_fall_back_to_root_catalog_runtime_config() {
        let root = loaded_catalog(
            "root",
            "/workspace-root/acowtancy",
            manifest_from_toml(
                r#"
[task_defaults]
run_in = "container"

[containers]
default = "workspace"

[containers.workspace]
primary_service = "workspace"
"#,
            ),
            0,
        );
        let child = loaded_catalog(
            "farmyard",
            "/workspace-root/acowtancy/farmyard",
            manifest_from_toml(
                r#"
[tasks.build]
run = "cargo test"
"#,
            ),
            1,
        );
        let catalogs = vec![root, child];
        let selection = TaskSelection {
            catalog: &catalogs[1],
            task: catalogs[1]
                .manifest
                .tasks
                .get("build")
                .expect("child task exists"),
            mode: CatalogSelectionMode::RootShallowest,
            evidence: vec!["test".to_owned()],
        };

        let (default_run_in, systems, containers) = effective_task_binding_inputs(
            Path::new("/workspace-root/acowtancy"),
            &catalogs,
            &selection,
        );

        assert_eq!(default_run_in, Some(ManifestTaskRunIn::Container));
        assert!(systems.is_none());
        let containers = containers.expect("root containers should be reused");
        assert_eq!(containers.default.as_deref(), Some("workspace"));
        assert!(containers.environments.contains_key("workspace"));
    }

    #[test]
    fn effective_task_binding_inputs_use_nearest_invocation_scope_catalog() {
        let root = loaded_catalog(
            "root",
            "/workspace-root/acowtancy",
            manifest_from_toml(
                r#"
[task_defaults]
run_in = "host"

[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"
"#,
            ),
            0,
        );
        let child = loaded_catalog(
            "farmyard",
            "/workspace-root/acowtancy/farmyard",
            manifest_from_toml(
                r#"
[task_defaults]
run_in = "container"

[containers]
default = "farmyard"

[containers.farmyard]
primary_service = "api"

[tasks.build]
run = "cargo test"
"#,
            ),
            1,
        );
        let catalogs = vec![root, child];
        let selection = TaskSelection {
            catalog: &catalogs[1],
            task: catalogs[1]
                .manifest
                .tasks
                .get("build")
                .expect("child task exists"),
            mode: CatalogSelectionMode::RootShallowest,
            evidence: vec!["test".to_owned()],
        };

        let (default_run_in, _systems, containers) = effective_task_binding_inputs(
            Path::new("/workspace-root/acowtancy/farmyard"),
            &catalogs,
            &selection,
        );

        assert_eq!(default_run_in, Some(ManifestTaskRunIn::Container));
        let containers = containers.expect("child containers should win in child scope");
        assert_eq!(containers.default.as_deref(), Some("farmyard"));
        assert!(containers.environments.contains_key("farmyard"));
    }

    #[test]
    fn effective_task_binding_inputs_merge_partial_child_containers_with_scope_root() {
        let root = loaded_catalog(
            "root",
            "/workspace-root/acowtancy",
            manifest_from_toml(
                r#"
[task_defaults]
run_in = "container"

[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"
"#,
            ),
            0,
        );
        let child = loaded_catalog(
            "farmyard",
            "/workspace-root/acowtancy/farmyard",
            manifest_from_toml(
                r#"
[containers.services.data]
pull_production = "rhai:scripts/tasks/pull-production-post-sql.rhai"

[tasks."db:migrate"]
run = "cargo run -p farmyard-db --bin migrate_dev_db"
"#,
            ),
            1,
        );
        let catalogs = vec![root, child];
        let selection = TaskSelection {
            catalog: &catalogs[1],
            task: catalogs[1]
                .manifest
                .tasks
                .get("db:migrate")
                .expect("child task exists"),
            mode: CatalogSelectionMode::ExplicitPrefix,
            evidence: vec!["test".to_owned()],
        };

        let (default_run_in, _systems, containers) = effective_task_binding_inputs(
            Path::new("/workspace-root/acowtancy"),
            &catalogs,
            &selection,
        );

        assert_eq!(default_run_in, Some(ManifestTaskRunIn::Container));
        let containers = containers.expect("merged containers should exist");
        assert_eq!(containers.default.as_deref(), Some("stack"));
        assert!(containers.environments.contains_key("stack"));
        assert!(containers.environments.contains_key("services"));
    }

    #[test]
    fn effective_task_binding_inputs_fall_back_to_ancestor_containers_from_child_scope() {
        let root = loaded_catalog(
            "root",
            "/workspace-root/acowtancy",
            manifest_from_toml(
                r#"
[containers]
default = "workspace"

[containers.workspace]
primary_service = "workspace"
"#,
            ),
            0,
        );
        let child = loaded_catalog(
            "cp-api",
            "/workspace-root/acowtancy/cp-api",
            manifest_from_toml(
                r#"
[tasks.build]
run_in = "container"
run = "cargo test"
"#,
            ),
            1,
        );
        let catalogs = vec![root, child];
        let selection = TaskSelection {
            catalog: &catalogs[1],
            task: catalogs[1]
                .manifest
                .tasks
                .get("build")
                .expect("child task exists"),
            mode: CatalogSelectionMode::RootShallowest,
            evidence: vec!["test".to_owned()],
        };

        let (_default_run_in, _systems, containers) = effective_task_binding_inputs(
            Path::new("/workspace-root/acowtancy/cp-api"),
            &catalogs,
            &selection,
        );

        let containers = containers.expect("ancestor containers should fill the child scope");
        assert_eq!(containers.default.as_deref(), Some("workspace"));
        assert!(containers.environments.contains_key("workspace"));
    }
}
