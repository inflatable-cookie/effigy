use super::*;
use crate::runner::execute::{resolve_container_execution_binding, ContainerExecutionBinding};
use effigy_containers::compose::{compose_args, compose_invocation};
use effigy_containers::load_container_exec_working_dir;
use effigy_containers::session::{
    managed_lifecycle_command, managed_shell_command, managed_standard_exec_command,
    resolve_effigy_invocation_prefix,
};
use effigy_managed::ManagedProcessRole;
use effigy_manifest::config_sections::ManifestJsPackageManager;
use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestSystemsConfig, TASK_MANIFEST_FILE,
};

const MANAGED_EXEC_READINESS_TIMEOUT_SECS: u64 = 30;

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
    let container_binding = resolve_container_execution_binding(
        systems,
        containers,
        task_name,
        task,
        "demo concurrent-runner task materialization",
    )?;
    let inline_policy = match &container_binding {
        ContainerExecutionBinding::Inline { .. } => {
            container_binding.load_effective_policy(repo_root)?
        }
        _ => None,
    };
    let container_repo_root = match &container_binding {
        ContainerExecutionBinding::Inline { .. } => {
            container_binding.exec_working_dir(repo_root)?
        }
        _ => container_binding
            .requested_container_name()
            .and_then(|requested_name| {
                load_container_exec_working_dir(repo_root, requested_name)
                    .map_err(|error| RunnerError::task_invocation(error.to_string()))
                    .ok()
            }),
    };
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

fn format_os_args(args: &[std::ffi::OsString]) -> String {
    args.iter()
        .map(|arg| shell_quote(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_inline_compose_command(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    args: &[std::ffi::OsString],
) -> String {
    let (program, resolved_args) = compose_invocation(policy, args);
    format!(
        "cd {} && {} {}",
        shell_quote(&repo_root.display().to_string()),
        shell_quote(program),
        format_os_args(&resolved_args),
    )
}

fn managed_lifecycle_state_path(
    repo_root: &Path,
    container_label: &str,
    owner_task: &str,
) -> std::path::PathBuf {
    repo_root
        .join(".effigy/runtime/managed-lifecycle")
        .join(format!(
            "{}-{}.state",
            sanitize_state_key(owner_task),
            sanitize_state_key(container_label)
        ))
}

fn sanitize_state_key(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn render_managed_lifecycle_setup_sequence(setup_commands: &[String]) -> String {
    if setup_commands.is_empty() {
        return String::new();
    }
    setup_commands
        .iter()
        .map(|command| {
            format!(
                "if ! {command}; then printf '%s\\n' 'managed lifecycle failed during container setup' 1>&2; exit 1; fi; "
            )
        })
        .collect()
}

fn render_inline_managed_lifecycle_command(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    owner_task: &str,
    health_wait: bool,
    ready_message: Option<&str>,
    dns_route_lines: &[String],
    setup_commands: &[String],
) -> String {
    let lifecycle_state = managed_lifecycle_state_path(repo_root, &policy.name, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let up = render_inline_compose_command(repo_root, policy, &compose_args(policy, ["up", "-d"]));
    let ps = render_inline_compose_command(repo_root, policy, &compose_args(policy, ["ps"]));
    let down = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["down", "--remove-orphans"]),
    );
    let readiness_status = if health_wait {
        "waiting for readiness via detached container startup"
    } else {
        "startup does not declare managed readiness waiting"
    };
    let ready_banner = ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("container `{}` is ready", policy.name));
    let dns_routes_section = render_managed_lifecycle_dns_routes_section(dns_route_lines);
    let setup_sequence = render_managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; started=0; cleanup() {{ if [ \"$started\" = 1 ]; then printf '%s\\n' stopped > \"$state_path\"; {down} >/dev/null 2>&1 || true; else printf '%s\\n' failed > \"$state_path\"; fi; }}; trap 'cleanup' EXIT INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; if ! {up}; then printf '%s\\n' 'managed lifecycle failed during container startup' 1>&2; exit 1; fi; started=1; {setup_sequence}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; {dns_routes_section}printf 'ready_message: %s\\n\\n' {ready_banner}; {ps} || true; printf '\\n[info] lifecycle owner is idle; use compose status to refresh.\\n'; {idle_wait}",
            label = shell_quote(&policy.name),
            owner_task = shell_quote(owner_task),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            dns_routes_section = dns_routes_section,
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        ))
    )
}

fn render_managed_lifecycle_dns_routes_section(dns_route_lines: &[String]) -> String {
    if dns_route_lines.is_empty() {
        return "printf 'dns_routes: none\\n\\n'; ".to_owned();
    }
    let mut section = "printf 'dns_routes:\\n'; ".to_owned();
    for line in dns_route_lines {
        section.push_str(&format!("printf '  - %s\\n' {}; ", shell_quote(line)));
    }
    section.push_str("printf '\\n'; ");
    section
}

fn render_inline_managed_shell_command(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    owner_task: &str,
    service: Option<&str>,
) -> String {
    let lifecycle_state = managed_lifecycle_state_path(repo_root, &policy.name, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let service_name = service
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(policy.primary_service.as_str());
    let readiness_probe = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["exec", service_name, "sh", "-lc", "true"]),
    );
    let attach = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(policy, ["exec", service_name, "sh"]),
    );
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; while true; do if {readiness_probe} >/dev/null 2>&1; then {attach}; exit $?; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before shell became available' 1>&2; exit 1; fi; sleep 1; done"
        ))
    )
}

fn managed_lifecycle_idle_wait_command() -> &'static str {
    "while kill -0 \"$parent_pid\" >/dev/null 2>&1; do sleep 1; done"
}

fn rewrite_command_for_container(
    command: &str,
    repo_root: &Path,
    container_repo_root: &Path,
) -> String {
    command.replace(
        &repo_root.display().to_string(),
        &container_repo_root.display().to_string(),
    )
}

fn container_exec_command(
    command: &str,
    repo_root: &Path,
    process_cwd: &Path,
    container_repo_root: Option<&Path>,
) -> String {
    let Some(container_repo_root) = container_repo_root else {
        return command.to_owned();
    };
    let container_cwd =
        container_repo_root.join(process_cwd.strip_prefix(repo_root).unwrap_or(process_cwd));
    let container_local_bin = container_cwd.join("node_modules/.bin");
    let rewritten_command = rewrite_command_for_container(command, repo_root, container_repo_root);
    format!(
        "export PATH={}:$PATH; cd {} && {}",
        shell_quote(&container_local_bin.display().to_string()),
        shell_quote(&container_cwd.display().to_string()),
        rewritten_command
    )
}

fn render_inline_managed_standard_exec_command(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    owner_task: &str,
    process_cwd: &Path,
    container_repo_root: Option<&Path>,
    setup_command: Option<&str>,
    command: &str,
) -> String {
    let cwd = shell_quote(&process_cwd.display().to_string());
    let lifecycle_state = managed_lifecycle_state_path(repo_root, &policy.name, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let probe = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(
            policy,
            [
                "exec",
                "-T",
                policy.primary_service.as_str(),
                "sh",
                "-lc",
                "true",
            ],
        ),
    );
    let rewritten = shell_quote(&container_exec_command(
        command,
        repo_root,
        process_cwd,
        container_repo_root,
    ));
    let attach = render_inline_compose_command(
        repo_root,
        policy,
        &compose_args(
            policy,
            [
                "exec",
                "-T",
                policy.primary_service.as_str(),
                "sh",
                "-lc",
                rewritten.as_str(),
            ],
        ),
    );
    let setup_sequence = setup_command.unwrap_or("");
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "cd {cwd} && state_path={lifecycle_state}; deadline=$(( $(date +%s) + {timeout_secs} )); while true; do if {probe} >/dev/null 2>&1; then {setup_sequence}{attach}; exit $?; fi; if [ -f \"$state_path\" ] && [ \"$(cat \"$state_path\")\" = failed ]; then printf '%s\\n' 'managed lifecycle failed before exec surface became available' 1>&2; exit 1; fi; if [ \"$(date +%s)\" -ge \"$deadline\" ]; then printf '%s\\n' 'managed exec timed out waiting for container exec readiness' 1>&2; exit 1; fi; sleep 1; done",
            timeout_secs = MANAGED_EXEC_READINESS_TIMEOUT_SECS,
            setup_sequence = setup_sequence,
        ))
    )
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    if value.bytes().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b':' | b'.' | b'_' | b'-'
        )
    }) {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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
        assert!(lifecycle.run.contains("down --remove-orphans"));
        assert!(standard.run.contains("exec -T workspace sh -lc"));
    }
}
