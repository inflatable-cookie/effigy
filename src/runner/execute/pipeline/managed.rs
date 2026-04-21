use super::super::super::container_command::run_task_workspace_session;
use super::super::super::container_command::support::validate_running_container_runtime_match;
use super::super::super::gateway_command::gateway_up_for_managed_task;
use super::super::super::locking::io::acquire_scopes;
use super::super::super::locking::model::LockScope;
use super::super::super::system_command::run_workspace_seeded_session;
use super::super::preflight::ExecutionPreflight;
use super::super::{resolve_container_execution_binding, ContainerExecutionBinding};
use crate::runner::error::RunnerError;
use crate::runner::util::render_passthrough_args;
use effigy_containers::compose::{compose_args, compose_invocation};
use effigy_containers::session::{
    managed_gateway_command, managed_lifecycle_command, managed_lifecycle_shutdown_command,
    managed_shell_command, managed_standard_exec_command, resolve_effigy_invocation_prefix,
};
use effigy_containers::{load_container_exec_working_dir, load_container_policy};
use effigy_managed::command::resolve_managed_task_plan;
use effigy_managed::presentation::run_or_render_managed_task;
use effigy_managed::ManagedProcessRole;
use effigy_managed::{managed_execution_mode, ManagedExecutionMode};
use effigy_manifest::TaskSelection;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const MANAGED_EXEC_READINESS_TIMEOUT_SECS: u64 = 30;
const MANAGED_LIFECYCLE_CLEANUP_TIMEOUT_SECS: u64 = 90;
const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";

pub(super) fn run_managed_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<Option<String>, RunnerError> {
    let container_handoff = inside_container_handoff();
    let execution_mode = managed_execution_mode();
    let container_binding = resolve_container_execution_binding(
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "managed task execution",
    )?;
    let plan = resolve_managed_task_plan(
        &preflight.selector,
        selection.catalog,
        selection.task,
        &preflight.runtime_args_exec,
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        &effigy_routing::resolve_task_selection,
    )?;
    let Some(mut plan) = plan else {
        if let ContainerExecutionBinding::Container { name, .. } = &container_binding {
            let repo_for_task = selection.catalog.catalog_root.clone();
            let lock_scopes = vec![crate::runner::manifest::task_lock_scope(
                selection.task,
                &preflight.selector,
            )];
            let _lock_guards = acquire_scopes(&preflight.resolved.resolved_root, &lock_scopes)?;
            return run_task_workspace_session(
                &repo_for_task,
                &preflight.selector.task_name,
                name.as_deref(),
                preflight.output_json,
            )
            .map(Some);
        }
        if matches!(container_binding, ContainerExecutionBinding::Inline { .. }) {
            return Err(RunnerError::task_invocation(format!(
                "task `{}` uses an inline workspace container, but non-managed attached container sessions do not support inline workspace containers yet",
                preflight.selector.task_name
            )));
        }
        return Ok(None);
    };
    if !container_handoff
        && execution_mode == ManagedExecutionMode::Tui
        && matches!(
            container_binding,
            ContainerExecutionBinding::Container { .. }
        )
    {
        let policy = load_container_policy(
            &selection.catalog.catalog_root,
            container_binding.requested_container_name().flatten(),
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        validate_running_container_runtime_match(&selection.catalog.catalog_root, &policy)?;
        maybe_start_managed_gateway(
            &selection.catalog.catalog_root,
            &preflight.selector.task_name,
            selection,
            plan.gateway_auto_start,
        )?;
        let seed_command = render_workspace_seeded_task_command(
            &preflight.selector.task_name,
            &preflight.runtime_args_exec.passthrough,
        );
        return run_workspace_seeded_session(
            &selection.catalog.catalog_root,
            container_binding.container_name(),
            preflight.runtime_args_raw.repo_override.clone(),
            &seed_command,
        )
        .map(Some);
    }
    materialize_special_managed_processes(&mut plan, preflight, selection, container_handoff)?;

    let repo_for_task = selection.catalog.catalog_root.clone();
    let mut lock_scopes = vec![crate::runner::manifest::task_lock_scope(
        selection.task,
        &preflight.selector,
    )];
    if selection.task.mode.as_deref() == Some("tui") {
        lock_scopes.push(LockScope::Profile {
            task: crate::runner::manifest::selector_lock_name(&preflight.selector),
            profile: plan.profile.clone(),
        });
    }
    let _lock_guards = acquire_scopes(&preflight.resolved.resolved_root, &lock_scopes)?;

    if execution_mode != ManagedExecutionMode::RenderPlan && !container_handoff {
        maybe_start_managed_gateway(
            &repo_for_task,
            &preflight.selector.task_name,
            selection,
            plan.gateway_auto_start,
        )?;
    }

    let lifecycle_cleanup = if !container_handoff
        && execution_mode != ManagedExecutionMode::RenderPlan
        && plan
            .processes
            .iter()
            .any(|process| process.role == ManagedProcessRole::Lifecycle)
    {
        Some(build_managed_lifecycle_cleanup_command(
            &repo_for_task,
            &container_binding,
        )?)
    } else {
        None
    };

    let result = run_or_render_managed_task(
        &preflight.selector.task_name,
        &repo_for_task,
        &selection.catalog.manifest_path,
        plan,
    );

    finish_managed_task(
        result.map(Some).map_err(Into::into),
        lifecycle_cleanup.as_deref(),
    )
}

fn build_managed_gateway_command(
    repo_root: &std::path::Path,
    task_name: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let container_binding = resolve_container_execution_binding(
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        task_name,
        selection.task,
        "managed gateway startup",
    )?;
    let requested_container_name =
        container_binding
            .requested_container_name()
            .ok_or_else(|| {
                RunnerError::task_invocation(
                    "`gateway = true` requires a workspace-backed container binding on the task",
                )
            })?;
    let policy = effigy_containers::load_container_policy(repo_root, requested_container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    if policy.dns_routes.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` sets `gateway = true`, but container session `{}` does not declare any `[containers.{}.dns].routes`",
            task_name,
            requested_container_name.unwrap_or("default"),
            policy.name
        )));
    }
    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    Ok(managed_gateway_command(&executable))
}

fn maybe_start_managed_gateway(
    repo_root: &std::path::Path,
    task_name: &str,
    selection: &TaskSelection<'_>,
    should_start: bool,
) -> Result<(), RunnerError> {
    if !should_start {
        return Ok(());
    }
    let gateway_command = build_managed_gateway_command(repo_root, task_name, selection)?;
    gateway_up_for_managed_task(&gateway_command)
}

fn normalize_managed_lifecycle_container_ref(container_name: &str) -> Option<&str> {
    match container_name.trim() {
        "" | "default" => None,
        other => Some(other),
    }
}

fn build_managed_lifecycle_cleanup_command(
    repo_root: &std::path::Path,
    container_binding: &ContainerExecutionBinding,
) -> Result<String, RunnerError> {
    match container_binding {
        ContainerExecutionBinding::Inline { .. } => {
            let policy = container_binding
                .load_effective_policy(repo_root)?
                .ok_or_else(|| {
                    RunnerError::task_invocation("missing inline workspace container policy")
                })?;
            Ok(render_inline_compose_command(
                repo_root,
                &policy,
                &compose_args(&policy, ["down", "--remove-orphans"]),
            ))
        }
        _ => {
            let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
            Ok(managed_lifecycle_shutdown_command(
                repo_root,
                container_binding
                    .container_name()
                    .and_then(normalize_managed_lifecycle_container_ref),
                &executable,
            ))
        }
    }
}

fn run_managed_lifecycle_cleanup(command: &str) -> Result<(), RunnerError> {
    println!("{}", render_managed_lifecycle_cleanup_notice(command));
    let mut child = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .spawn()
        .map_err(RunnerError::Cwd)?;
    let deadline = Instant::now() + Duration::from_secs(MANAGED_LIFECYCLE_CLEANUP_TIMEOUT_SECS);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(RunnerError::Cwd)? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunnerError::task_invocation(format!(
                "managed lifecycle cleanup timed out after {}s: `{command}`",
                MANAGED_LIFECYCLE_CLEANUP_TIMEOUT_SECS
            )));
        }
        thread::sleep(Duration::from_millis(100));
    };
    if status.success() {
        Ok(())
    } else {
        Err(RunnerError::task_invocation(format!(
            "managed lifecycle cleanup failed: `{command}` exited with {status}"
        )))
    }
}

fn render_managed_lifecycle_cleanup_notice(command: &str) -> String {
    format!("[info] waiting for container shutdown: `{command}`")
}

fn finish_managed_task(
    task_result: Result<Option<String>, RunnerError>,
    cleanup_command: Option<&str>,
) -> Result<Option<String>, RunnerError> {
    let cleanup_result = if task_result.is_ok() {
        cleanup_command
            .map(run_managed_lifecycle_cleanup)
            .transpose()
            .map(|_| ())
    } else {
        Ok(())
    };
    match (task_result, cleanup_result) {
        (Ok(output), Ok(())) => Ok(output),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(task_error), Ok(())) => Err(task_error),
        (Err(task_error), Err(cleanup_error)) => Err(RunnerError::task_invocation(format!(
            "{task_error}\nmanaged lifecycle cleanup also failed: {cleanup_error}"
        ))),
    }
}

fn materialize_special_managed_processes(
    plan: &mut effigy_managed::ManagedTaskPlan,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    container_handoff: bool,
) -> Result<(), RunnerError> {
    if !plan
        .processes
        .iter()
        .any(|process| process.role == ManagedProcessRole::Lifecycle)
    {
        return Ok(());
    }

    let executable = resolve_effigy_invocation_prefix().map_err(RunnerError::Cwd)?;
    let repo_root = selection.catalog.catalog_root.as_path();
    let container_binding = resolve_container_execution_binding(
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "managed process materialization",
    )?;
    let inline_policy = match &container_binding {
        ContainerExecutionBinding::Inline { .. } => {
            container_binding.load_effective_policy(repo_root)?
        }
        _ => None,
    };
    if !container_handoff {
        if let Some(policy) = inline_policy.as_ref() {
            validate_running_container_runtime_match(repo_root, policy)?;
        } else if let Some(requested_container_name) =
            container_binding.requested_container_name().flatten()
        {
            let policy = load_container_policy(repo_root, Some(requested_container_name))
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            validate_running_container_runtime_match(repo_root, &policy)?;
        }
    }
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
    for process in &mut plan.processes {
        match process.role {
            ManagedProcessRole::Lifecycle => {
                process.run = if container_handoff {
                    render_handoff_managed_lifecycle_command(
                        repo_root,
                        container_binding.container_name().unwrap_or("default"),
                        &preflight.selector.task_name,
                        selection.task.health_wait.unwrap_or(false),
                        selection.task.ready_message.as_deref(),
                        &[],
                    )
                } else if let Some(policy) = inline_policy.as_ref() {
                    render_inline_managed_lifecycle_command(
                        repo_root,
                        policy,
                        &preflight.selector.task_name,
                        selection.task.health_wait.unwrap_or(false),
                        selection.task.ready_message.as_deref(),
                        &[],
                    )
                } else {
                    managed_lifecycle_command(
                        repo_root,
                        container_binding.container_name(),
                        &preflight.selector.task_name,
                        selection.task.health_wait.unwrap_or(false),
                        selection.task.ready_message.as_deref(),
                        &[],
                        &executable,
                    )
                };
            }
            ManagedProcessRole::Shell => {
                process.run = if container_handoff {
                    selection
                        .catalog
                        .manifest
                        .shell
                        .as_ref()
                        .and_then(|shell| shell.run.clone())
                        .unwrap_or_else(default_handoff_managed_shell_run)
                } else if let Some(policy) = inline_policy.as_ref() {
                    render_inline_managed_shell_command(
                        repo_root,
                        policy,
                        &preflight.selector.task_name,
                        process.service.as_deref(),
                    )
                } else {
                    managed_shell_command(
                        repo_root,
                        container_binding.container_name(),
                        &preflight.selector.task_name,
                        process.service.as_deref(),
                        &executable,
                    )
                };
            }
            ManagedProcessRole::Standard => {
                if container_handoff {
                    process.run = render_handoff_managed_standard_command(
                        process.setup.as_deref(),
                        &process.run,
                    );
                    continue;
                }
                if let Some(policy) = inline_policy.as_ref() {
                    process.run = render_inline_managed_standard_exec_command(
                        repo_root,
                        policy,
                        &preflight.selector.task_name,
                        &process.cwd,
                        container_repo_root.as_deref(),
                        process.setup.as_deref(),
                        &process.run,
                    );
                } else if container_binding.container_name().is_some() {
                    process.run = managed_standard_exec_command(
                        repo_root,
                        container_binding.container_name(),
                        &preflight.selector.task_name,
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

fn inside_container_handoff() -> bool {
    std::env::var_os(CONTAINER_HANDOFF_ENV).is_some()
}

fn render_handoff_managed_standard_command(setup_command: Option<&str>, run: &str) -> String {
    let Some(setup_command) = setup_command
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return run.to_owned();
    };
    format!(
        "if ! {setup_command}; then printf '%s\\n' 'managed setup failed before process launch' 1>&2; exit 1; fi; {run}"
    )
}

fn format_os_args(args: &[std::ffi::OsString]) -> String {
    args.iter()
        .map(|arg| shell_quote(&arg.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_inline_compose_command(
    repo_root: &std::path::Path,
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
    repo_root: &std::path::Path,
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

fn render_handoff_managed_lifecycle_command(
    repo_root: &std::path::Path,
    container_label: &str,
    owner_task: &str,
    health_wait: bool,
    ready_message: Option<&str>,
    setup_commands: &[String],
) -> String {
    let lifecycle_state = managed_lifecycle_state_path(repo_root, container_label, owner_task);
    let lifecycle_state = shell_quote(&lifecycle_state.display().to_string());
    let readiness_status = if health_wait {
        "workspace container is already running in handoff mode"
    } else {
        "running inside workspace container handoff"
    };
    let ready_banner = ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("workspace container `{container_label}` is ready"));
    let setup_sequence = render_managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; cleanup() {{ printf '%s\\n' stopped > \"$state_path\"; }}; trap 'cleanup' EXIT INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; {setup_sequence}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; printf 'ready_message: %s\\n\\n' {ready_banner}; printf '[info] lifecycle owner is idle; workspace container handoff is already active.\\n'; {idle_wait}",
            label = shell_quote(container_label),
            owner_task = shell_quote(owner_task),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        ))
    )
}

fn render_inline_managed_lifecycle_command(
    repo_root: &std::path::Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    owner_task: &str,
    health_wait: bool,
    ready_message: Option<&str>,
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
    let setup_sequence = render_managed_lifecycle_setup_sequence(setup_commands);
    let idle_wait = managed_lifecycle_idle_wait_command();
    format!(
        "sh -lc {}",
        shell_quote(&format!(
            "state_path={lifecycle_state}; parent_pid=$PPID; mkdir -p \"$(dirname \"$state_path\")\"; printf '%s\\n' starting > \"$state_path\"; started=0; cleanup() {{ if [ \"$started\" = 1 ]; then printf '%s\\n' stopped > \"$state_path\"; {down} >/dev/null 2>&1 || true; else printf '%s\\n' failed > \"$state_path\"; fi; }}; trap 'cleanup' EXIT INT TERM; printf 'managed lifecycle: %s\\n' {readiness_status}; if ! {up}; then printf '%s\\n' 'managed lifecycle failed during container startup' 1>&2; exit 1; fi; started=1; {setup_sequence}printf '%s\\n' ready > \"$state_path\"; printf 'managed ready: %s\\n' {ready_banner}; printf 'Managed Container Lifecycle\\n\\n'; printf 'container: %s\\n' {label}; printf 'owner_task: %s\\n' {owner_task}; printf 'readiness: %s\\n' {readiness_status}; printf 'ready_message: %s\\n\\n' {ready_banner}; {ps} || true; printf '\\n[info] lifecycle owner is idle; use compose status to refresh.\\n'; {idle_wait}",
            label = shell_quote(&policy.name),
            owner_task = shell_quote(owner_task),
            readiness_status = shell_quote(readiness_status),
            ready_banner = shell_quote(&ready_banner),
            setup_sequence = setup_sequence,
            idle_wait = idle_wait,
        ))
    )
}

fn render_inline_managed_shell_command(
    repo_root: &std::path::Path,
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
    repo_root: &std::path::Path,
    container_repo_root: &std::path::Path,
) -> String {
    command.replace(
        &repo_root.display().to_string(),
        &container_repo_root.display().to_string(),
    )
}

fn container_exec_command(
    command: &str,
    repo_root: &std::path::Path,
    process_cwd: &std::path::Path,
    container_repo_root: Option<&std::path::Path>,
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
    repo_root: &std::path::Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    owner_task: &str,
    process_cwd: &std::path::Path,
    container_repo_root: Option<&std::path::Path>,
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

fn default_handoff_managed_shell_run() -> String {
    "if [ -n \"${SHELL:-}\" ] && [ -x \"${SHELL}\" ]; then exec \"${SHELL}\" -i; fi; if command -v bash >/dev/null 2>&1; then exec \"$(command -v bash)\" -i; fi; if command -v sh >/dev/null 2>&1; then exec \"$(command -v sh)\" -i; fi; exec /bin/sh -i".to_owned()
}

fn render_workspace_seeded_task_command(task_name: &str, args: &[String]) -> String {
    let mut rendered = format!("effigy {}", shell_quote(task_name));
    let rendered_args = render_passthrough_args(args);
    if !rendered_args.is_empty() {
        rendered.push(' ');
        rendered.push_str(&rendered_args);
    }
    rendered
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
    use super::{
        default_handoff_managed_shell_run, finish_managed_task,
        render_handoff_managed_standard_command, render_inline_managed_standard_exec_command,
        render_managed_lifecycle_cleanup_notice, render_workspace_seeded_task_command,
    };
    use crate::runner::error::RunnerError;
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::Path;

    #[test]
    fn finish_managed_task_preserves_primary_failure_without_running_cleanup() {
        let error = finish_managed_task(
            Err(RunnerError::task_invocation("task failed")),
            Some("missing-cleanup-command >/dev/null 2>&1"),
        )
        .expect_err("primary task failure should surface");

        let rendered = error.to_string();
        assert!(rendered.contains("task failed"), "got: {rendered}");
        assert!(
            !rendered.contains("managed lifecycle cleanup also failed"),
            "got: {rendered}"
        );
    }

    #[test]
    fn managed_lifecycle_cleanup_notice_is_stable() {
        let rendered =
            render_managed_lifecycle_cleanup_notice("effigy container stack down --repo /tmp/demo");

        assert_eq!(
            rendered,
            "[info] waiting for container shutdown: `effigy container stack down --repo /tmp/demo`"
        );
    }

    #[test]
    fn handoff_shell_default_avoids_hardcoded_zsh_requirement() {
        let rendered = default_handoff_managed_shell_run();

        assert!(rendered.contains("exec \"${SHELL}\" -i"), "got: {rendered}");
        assert!(rendered.contains("command -v bash"), "got: {rendered}");
        assert!(rendered.contains("command -v sh"), "got: {rendered}");
        assert!(rendered.contains("exec /bin/sh -i"), "got: {rendered}");
        assert!(!rendered.contains("/bin/zsh"), "got: {rendered}");
    }

    #[test]
    fn workspace_seeded_task_command_preserves_passthrough_args() {
        let rendered = render_workspace_seeded_task_command(
            "dev",
            &[
                "front".to_owned(),
                "--".to_owned(),
                "--host".to_owned(),
                "0.0.0.0".to_owned(),
            ],
        );

        assert_eq!(rendered, "effigy dev 'front' '--' '--host' '0.0.0.0'");
    }

    #[test]
    fn handoff_managed_standard_command_runs_setup_before_process() {
        let rendered =
            render_handoff_managed_standard_command(Some("printf setup-ok"), "bun run dev");

        let setup_index = rendered.find("printf setup-ok").expect("setup command");
        let run_index = rendered.find("bun run dev").expect("run command");
        assert!(setup_index < run_index, "got: {rendered}");
        assert!(
            rendered.contains("managed setup failed before process launch"),
            "got: {rendered}"
        );
    }

    #[test]
    fn inline_managed_standard_exec_runs_setup_before_attach() {
        let rendered = render_inline_managed_standard_exec_command(
            Path::new("/tmp/repo"),
            &test_policy(),
            "dev",
            Path::new("/tmp/repo/acme-front"),
            Some(Path::new("/workspace-root/repo")),
            Some("printf setup-ok; "),
            "bun run dev",
        );

        let setup_index = rendered.find("printf setup-ok").expect("setup command");
        let attach_index = rendered
            .find("bun run dev")
            .expect("attach command should be present");
        assert!(setup_index < attach_index, "got: {rendered}");
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "stack".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Attached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "stack".to_owned(),
            primary_service: "workspace".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }
}
