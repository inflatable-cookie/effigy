use super::*;
use crate::runner::execute::api::{
    resolve_execution_binding_resolution, ContainerExecutionBinding,
};
use crate::runner::managed_shell::{
    managed_readiness_probe_urls, render_inline_compose_command,
    render_inline_managed_lifecycle_command, render_inline_managed_shell_command,
    render_inline_managed_standard_exec_command,
};
use effigy_containers::compose::compose_args;
use effigy_containers::session::{
    managed_lifecycle_command, managed_shell_command, managed_standard_exec_command,
    resolve_effigy_invocation_prefix,
};
use effigy_core::shell::shell_quote;
use effigy_managed::ManagedProcessRole;
use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestSystemsConfig, TASK_MANIFEST_FILE,
};

pub(in crate::runner::demo_command) struct DemoTaskSelectionResolved {
    selector: TaskSelector,
    catalogs: Vec<LoadedCatalog>,
    selected_catalog_index: usize,
}

impl DemoTaskSelectionResolved {
    pub(super) fn selection(&self) -> Result<TaskSelection<'_>, RunnerError> {
        select_catalog_and_task(
            &self.selector,
            &self.catalogs,
            &self.catalogs[self.selected_catalog_index].catalog_root,
        )
        .map_err(Into::into)
    }

    pub(in crate::runner::demo_command) fn task(&self) -> Result<&ManifestTask, RunnerError> {
        self.selection().map(|selection| selection.task)
    }
}

pub(in crate::runner::demo_command) fn demo_task_selection(
    repo_root: &Path,
    task_name: &str,
) -> Result<Option<DemoTaskSelectionResolved>, RunnerError> {
    let catalogs = effigy_routing::discover_catalogs_allow_missing(repo_root)?;
    if catalogs.is_empty() {
        return Ok(None);
    }
    let selector = parse_task_selector(task_name).map_err(RunnerError::task_invocation)?;
    let selection = select_catalog_and_task(&selector, &catalogs, repo_root)?;
    let selected_catalog_index = catalogs
        .iter()
        .position(|catalog| {
            catalog.alias == selection.catalog.alias
                && catalog.catalog_root == selection.catalog.catalog_root
                && catalog.manifest_path == selection.catalog.manifest_path
        })
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "failed to re-identify selected task catalog for demo task `{task_name}`"
            ))
        })?;
    Ok(Some(DemoTaskSelectionResolved {
        selector,
        catalogs,
        selected_catalog_index,
    }))
}

pub(in crate::runner::demo_command) fn concurrent_runner_task_process_names(
    repo_root: &Path,
    task_name: &str,
) -> Option<Vec<String>> {
    let Ok(Some(resolved)) = demo_task_selection(repo_root, task_name) else {
        return None;
    };
    let Ok(selection) = resolved.selection() else {
        return None;
    };
    let runtime_args = TaskRuntimeArgs {
        repo_override: None,
        verbose_root: false,
        env_schema_override: None,
        passthrough: Vec::new(),
    };
    resolve_managed_task_plan(
        &resolved.selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &resolved.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )
    .ok()
    .flatten()
    .map(|plan| {
        plan.processes
            .iter()
            .map(|process| process.name.clone())
            .collect()
    })
}

pub(super) fn resolve_concurrent_runner_plan(
    resolved: &DemoTaskSelectionResolved,
    selection: TaskSelection<'_>,
    demo_id: &str,
    task_name: &str,
) -> Result<effigy_managed::ManagedTaskPlan, RunnerError> {
    let runtime_args = TaskRuntimeArgs {
        repo_override: None,
        verbose_root: false,
        env_schema_override: None,
        passthrough: Vec::new(),
    };
    let mut plan = resolve_managed_task_plan(
        &resolved.selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &resolved.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )?
    .ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "demo `{demo_id}` task `{task_name}` does not resolve to a managed concurrent runtime"
        ))
    })?;
    materialize_demo_special_managed_processes(
        &mut plan,
        selection.catalog.catalog_root.as_path(),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        selection.task,
        &resolved.selector.task_name,
    )?;
    Ok(plan)
}

fn materialize_demo_special_managed_processes(
    plan: &mut effigy_managed::ManagedTaskPlan,
    repo_root: &Path,
    systems: Option<&ManifestSystemsConfig>,
    containers: Option<&effigy_manifest::ManifestContainersConfig>,
    task: &ManifestTask,
    task_name: &str,
) -> Result<(), RunnerError> {
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let binding_resolution = resolve_execution_binding_resolution(
        None,
        systems,
        containers,
        task_name,
        task,
        "demo concurrent-runner task materialization",
    )?;
    let container_binding = binding_resolution.binding();
    let inline_policy = if binding_resolution.is_inline_container() {
        binding_resolution.effective_policy(repo_root)?
    } else {
        None
    };
    let container_repo_root = binding_resolution.exec_working_dir(repo_root)?;
    let lifecycle_setup_commands = build_demo_managed_lifecycle_setup_commands(
        plan,
        repo_root,
        container_repo_root.as_deref(),
        inline_policy.as_ref(),
        &executable,
    )?;
    for process in &mut plan.processes {
        match process.role {
            ManagedProcessRole::Lifecycle => {
                process.run = if let Some(policy) = inline_policy.as_ref() {
                    render_inline_managed_lifecycle_command(
                        repo_root,
                        policy,
                        task_name,
                        task.health_wait.unwrap_or(false),
                        task.ready_message.as_deref(),
                        &[],
                        &managed_readiness_probe_urls(inline_policy.as_ref()),
                        &lifecycle_setup_commands,
                    )
                } else {
                    managed_lifecycle_command(
                        repo_root,
                        container_binding.container_name(),
                        task_name,
                        task.health_wait.unwrap_or(false),
                        task.ready_message.as_deref(),
                        &[],
                        &[],
                        &lifecycle_setup_commands,
                        &executable,
                    )
                };
            }
            ManagedProcessRole::Shell => {
                process.run = if let Some(policy) = inline_policy.as_ref() {
                    render_inline_managed_shell_command(
                        repo_root,
                        policy,
                        task_name,
                        process.service.as_deref(),
                    )
                } else {
                    managed_shell_command(
                        repo_root,
                        container_binding.container_name(),
                        task_name,
                        process.service.as_deref(),
                        &executable,
                    )
                };
            }
            ManagedProcessRole::Standard => {
                if process.run_on_host {
                    if let Some(setup) = process.setup.as_deref() {
                        process.run = format!("{setup}\n{}", process.run);
                    }
                    continue;
                }
                if let Some(policy) = inline_policy.as_ref() {
                    process.run = render_inline_managed_standard_exec_command(
                        repo_root,
                        policy,
                        task_name,
                        &process.cwd,
                        container_repo_root.as_deref(),
                        process.setup.as_deref(),
                        &process.run,
                    );
                } else if matches!(
                    container_binding,
                    ContainerExecutionBinding::Container { .. }
                ) {
                    process.run = managed_standard_exec_command(
                        repo_root,
                        container_binding.container_name(),
                        task_name,
                        &process.cwd,
                        container_repo_root.as_deref(),
                        process.setup.as_deref(),
                        &executable,
                        &process.run,
                    );
                }
            }
        }
    }
    Ok(())
}

fn build_demo_managed_lifecycle_setup_commands(
    plan: &effigy_managed::ManagedTaskPlan,
    repo_root: &Path,
    container_repo_root: Option<&Path>,
    inline_policy: Option<&effigy_containers::EffectiveContainerPolicy>,
    executable: &str,
) -> Result<Vec<String>, RunnerError> {
    let Some(container_repo_root) = container_repo_root else {
        return Ok(Vec::new());
    };
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let Some(package_manager) = loaded
        .manifest
        .package_manager
        .as_ref()
        .and_then(|config| config.js)
    else {
        return Ok(Vec::new());
    };
    let Some(install_command) = package_manager.install_command() else {
        return Ok(Vec::new());
    };
    let mut setup_dirs = std::collections::BTreeSet::<std::path::PathBuf>::new();
    for process in &plan.processes {
        if process.role != ManagedProcessRole::Standard {
            continue;
        }
        if process.cwd.join("package.json").is_file() {
            setup_dirs.insert(process.cwd.clone());
        }
    }
    Ok(setup_dirs
        .into_iter()
        .filter_map(|host_dir| {
            let relative_dir = host_dir.strip_prefix(repo_root).ok()?;
            let container_dir = container_repo_root.join(relative_dir);
            Some(if let Some(policy) = inline_policy {
                render_inline_container_js_hydration_command(
                    repo_root,
                    policy,
                    &container_dir,
                    package_manager,
                    install_command,
                )
            } else {
                render_container_js_hydration_command(
                    repo_root,
                    &container_dir,
                    package_manager,
                    install_command,
                    executable,
                )
            })
        })
        .collect())
}

fn render_container_js_hydration_command(
    repo_root: &Path,
    container_dir: &Path,
    package_manager: ManifestJsPackageManager,
    install_command: &str,
    executable: &str,
) -> String {
    let repo = shell_quote(&repo_root.display().to_string());
    let container_dir = shell_quote(&container_dir.display().to_string());
    let package_manager_label = package_manager.binary_name().unwrap_or("js");
    let script = format!(
        "cd {container_dir} && if [ -f package.json ] && {{ [ ! -d node_modules ] || [ -z \"$(ls -A node_modules 2>/dev/null)\" ]; }}; then printf 'managed setup: hydrating %s in %s\\n' {package_manager_label} {container_dir}; {install_command}; fi",
    );
    format!(
        "{executable} exec --repo {repo} -- sh -lc {script}",
        script = shell_quote(&script),
    )
}

fn render_inline_container_js_hydration_command(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    container_dir: &Path,
    package_manager: ManifestJsPackageManager,
    install_command: &str,
) -> String {
    let container_dir = shell_quote(&container_dir.display().to_string());
    let package_manager_label = package_manager.binary_name().unwrap_or("js");
    let script = format!(
        "cd {container_dir} && if [ -f package.json ] && {{ [ ! -d node_modules ] || [ -z \"$(ls -A node_modules 2>/dev/null)\" ]; }}; then printf 'managed setup: hydrating %s in %s\\n' {package_manager_label} {container_dir}; {install_command}; fi",
    );
    render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(
            policy,
            [
                "exec",
                policy.primary_service.as_str(),
                "sh",
                "-lc",
                script.as_str(),
            ],
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_test_support::temp_workspace;
    use std::fs;

    #[test]
    fn resolve_concurrent_runner_plan_supports_inline_workspace_binding() {
        let root = temp_workspace("demo-inline-workspace-binding");
        fs::write(
            root.join(TASK_MANIFEST_FILE),
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "window", run = "true", start = 2, tab = 2, shutdown_on_exit = true }
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
working_dir = "."
container = { image = "alpine:latest", mount = "./:/workspace" }
"#,
        )
        .expect("write manifest");

        let resolved = demo_task_selection(&root, "dev")
            .expect("resolve demo selection")
            .expect("selection");
        let selection = resolved.selection().expect("selection detail");
        let plan = resolve_concurrent_runner_plan(&resolved, selection, "demo-inline", "dev")
            .expect("resolve concurrent runner plan");

        let lifecycle = plan
            .processes
            .iter()
            .find(|process| process.role == ManagedProcessRole::Lifecycle)
            .expect("lifecycle process");
        let standard = plan
            .processes
            .iter()
            .find(|process| process.role == ManagedProcessRole::Standard)
            .expect("standard process");

        assert!(lifecycle
            .run
            .contains(".effigy/inline-workspaces/dev__app/docker-compose.yml"));
        assert!(lifecycle.run.contains("'down'") && lifecycle.run.contains("'--remove-orphans'"));
        assert!(
            standard.run.contains("'exec'")
                && standard.run.contains("'-T'")
                && standard.run.contains("'workspace'")
                && standard.run.contains("'sh'")
                && standard.run.contains("'-lc'")
        );
    }

    #[test]
    fn concurrent_entry_run_in_host_skips_container_wrap() {
        let root = temp_workspace("demo-concurrent-run-in-host");
        fs::write(
            root.join(TASK_MANIFEST_FILE),
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", start = 1, tab = 1 },
  { name = "in-container", run = "echo container", start = 2, tab = 2 },
  { name = "host-sidecar", run = "echo host", run_in = "host", start = 3, tab = 3 },
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
working_dir = "."
container = { image = "alpine:latest", mount = "./:/workspace" }
"#,
        )
        .expect("write manifest");

        let resolved = demo_task_selection(&root, "dev")
            .expect("resolve demo selection")
            .expect("selection");
        let selection = resolved.selection().expect("selection detail");
        let plan = resolve_concurrent_runner_plan(&resolved, selection, "demo-host", "dev")
            .expect("resolve concurrent runner plan");

        let in_container = plan
            .processes
            .iter()
            .find(|process| process.name == "in-container")
            .expect("in-container process");
        let host_sidecar = plan
            .processes
            .iter()
            .find(|process| process.name == "host-sidecar")
            .expect("host-sidecar process");

        assert!(
            in_container.run.contains("'exec'") && in_container.run.contains("'workspace'"),
            "default concurrent entry should still get the container wrap; got: {}",
            in_container.run
        );
        assert_eq!(
            host_sidecar.run.trim(),
            "echo host",
            "`run_in = \"host\"` entry should run the raw command on the host (no compose exec wrap)"
        );
    }

    #[test]
    fn concurrent_entry_run_in_host_rejected_on_lifecycle_role() {
        let root = temp_workspace("demo-concurrent-run-in-host-lifecycle");
        fs::write(
            root.join(TASK_MANIFEST_FILE),
            r#"[tasks.dev]
mode = "tui"
workspace = "app"
container_lifecycle = true
concurrent = [
  { role = "lifecycle", run_in = "host", start = 1, tab = 1 },
]

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
working_dir = "."
container = { image = "alpine:latest", mount = "./:/workspace" }
"#,
        )
        .expect("write manifest");

        let resolved = demo_task_selection(&root, "dev")
            .expect("resolve demo selection")
            .expect("selection");
        let selection = resolved.selection().expect("selection detail");
        let err = resolve_concurrent_runner_plan(&resolved, selection, "demo-host-bad", "dev")
            .expect_err("expected run_in = host on lifecycle role to error");
        let message = format!("{err}");
        assert!(
            message.contains("run_in = \"host\"") && message.contains("standard"),
            "error should mention the directive and that only standard entries support it; got: {message}"
        );
    }
}
