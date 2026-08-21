use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::Path;
use std::process::Output;

use effigy_cli::ExecArgs;
use effigy_containers::EffectiveContainerPolicy;
use effigy_env::secret::SecretString;
use effigy_exec::detection::determine_strategy;
use effigy_manifest::ManifestContainerConfig;
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeActivationRoute};
use effigy_tasks::{render_task_selector, TaskSelector};

use super::command_context::resolve_active_command_context;
use super::container_runtime_prep::{
    activate_container_runtime_for_task, build_runtime_activation_plan, ActivationRequest,
    ContainerTaskActivation,
};
use super::error::RunnerError;
use super::host_container_lease::emit_host_container_lease_notice;
use super::runtime_session_context::{current_runtime_session_context, RuntimeSessionContext};
use super::system_command::ensure_workspace_effigy_available_for_policy;
use surface::{
    build_alias_table, build_raw_exec_args, ensure_container_running, exec_alias_surface_absent,
    resolve_default_exec_surface, resolve_exec_working_dir, resolve_running_named_exec_surface,
    RawExecOptions, ResolvedExecSurface,
};
use transport::build_routed_task_exec_args;

mod surface;
mod transport;

pub(in crate::runner) use transport::{
    append_color_exec_env, copy_file_into_service, probe_container_capabilities, run_compose_exec,
    run_compose_exec_plan_with_options,
};

pub(super) fn run_exec(args: ExecArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override)?;
    run_explicit_exec(
        &context.resolved.resolved_root,
        &context.invocation_cwd,
        args.service.as_deref(),
        &args.command,
        args.output_json,
    )
}

pub(in crate::runner) fn try_run_exec_alias(
    repo_root: &Path,
    invocation_cwd: &Path,
    alias_name: &str,
    extra_args: &[String],
    output_json: bool,
) -> Result<Option<String>, RunnerError> {
    let surface = match resolve_default_exec_surface(repo_root) {
        Ok(surface) => surface,
        Err(error) if exec_alias_surface_absent(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let alias_table = build_alias_table(&surface.config)?;
    let alias = match alias_table.resolve_command(alias_name, extra_args) {
        Ok(alias) => alias,
        Err(effigy_exec::ExecError::AliasNotFound { .. }) => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation(error.to_string())),
    };
    let activation = activate_exec_surface(repo_root, &surface)?;

    let output = run_raw_exec(RawExecRequest {
        repo_root,
        invocation_cwd,
        container_name: &surface.container_name,
        config: &surface.config,
        policy: &surface.policy,
        service: &alias.service,
        command: &alias.command,
        output_json,
        alias_name: Some(alias_name),
    })?;
    maybe_emit_exec_activation_notice(&surface, activation);
    Ok(Some(output))
}

pub(in crate::runner) struct RoutedTaskExecRequest<'a> {
    pub repo_root: &'a Path,
    pub invocation_cwd: &'a Path,
    pub selector: &'a TaskSelector,
    pub task_args: &'a [String],
    pub service: &'a str,
    pub command: &'a str,
    pub task_env: Option<&'a BTreeMap<String, String>>,
    pub secret_env: Option<&'a [(&'a str, &'a SecretString)]>,
}

pub(in crate::runner) fn run_routed_task_container_exec(
    request: RoutedTaskExecRequest<'_>,
    container_name: &str,
) -> Result<String, RunnerError> {
    let output = run_routed_task_container_exec_variant(
        &request,
        RoutedTaskExecSurface::Named { container_name },
        false,
    )?;

    finish_routed_task_exec(request.command, output)
}

pub(in crate::runner) fn capture_routed_task_container_exec(
    request: RoutedTaskExecRequest<'_>,
    container_name: &str,
) -> Result<Output, RunnerError> {
    run_routed_task_container_exec_variant(
        &request,
        RoutedTaskExecSurface::Named { container_name },
        true,
    )
}

pub(in crate::runner) fn run_routed_task_container_exec_with_policy(
    request: RoutedTaskExecRequest<'_>,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<String, RunnerError> {
    let output = run_routed_task_container_exec_variant(
        &request,
        RoutedTaskExecSurface::ResolvedPolicy {
            policy,
            working_dir,
        },
        false,
    )?;

    finish_routed_task_exec(request.command, output)
}

pub(in crate::runner) fn capture_routed_task_container_exec_with_policy(
    request: RoutedTaskExecRequest<'_>,
    policy: &EffectiveContainerPolicy,
    working_dir: &Path,
) -> Result<Output, RunnerError> {
    run_routed_task_container_exec_variant(
        &request,
        RoutedTaskExecSurface::ResolvedPolicy {
            policy,
            working_dir,
        },
        true,
    )
}

fn run_explicit_exec(
    repo_root: &Path,
    invocation_cwd: &Path,
    service_override: Option<&str>,
    command: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let surface = resolve_default_exec_surface(repo_root)?;
    let activation = activate_exec_surface(repo_root, &surface)?;
    let service = service_override.unwrap_or(surface.policy.primary_service.as_str());
    let output = run_raw_exec(RawExecRequest {
        repo_root,
        invocation_cwd,
        container_name: &surface.container_name,
        config: &surface.config,
        policy: &surface.policy,
        service,
        command,
        output_json,
        alias_name: None,
    })?;
    maybe_emit_exec_activation_notice(&surface, activation);
    Ok(output)
}

fn activate_exec_surface(
    repo_root: &Path,
    surface: &ResolvedExecSurface,
) -> Result<ContainerTaskActivation, RunnerError> {
    activate_exec_surface_with(repo_root, surface, |repo_root, surface, plan| {
        activate_container_runtime_for_task(
            repo_root,
            &surface.policy,
            ActivationRequest {
                container_name: Some(surface.container_name.as_str()),
                repo_override: plan.request.repo_override.clone(),
                route: plan.route,
                session_context: current_runtime_session_context(),
            },
        )
    })
}

pub(super) fn activate_exec_surface_with(
    repo_root: &Path,
    surface: &ResolvedExecSurface,
    activate: impl FnOnce(
        &Path,
        &ResolvedExecSurface,
        &RuntimeActivationPlan,
    ) -> Result<ContainerTaskActivation, RunnerError>,
) -> Result<ContainerTaskActivation, RunnerError> {
    let plan = exec_runtime_activation_plan(repo_root, surface, current_runtime_session_context());
    activate(repo_root, surface, &plan)
}

fn exec_runtime_activation_plan(
    repo_root: &Path,
    surface: &ResolvedExecSurface,
    session_context: RuntimeSessionContext,
) -> RuntimeActivationPlan {
    build_runtime_activation_plan(
        repo_root,
        &surface.policy.name,
        Some(surface.container_name.as_str()),
        Some(repo_root.to_path_buf()),
        RuntimeActivationRoute::Exec,
        session_context,
    )
}

fn maybe_emit_exec_activation_notice(
    surface: &ResolvedExecSurface,
    activation: ContainerTaskActivation,
) {
    if activation.refreshed_host_container_lease {
        emit_host_container_lease_notice(&surface.container_name);
    }
}

struct RawExecRequest<'a> {
    repo_root: &'a Path,
    invocation_cwd: &'a Path,
    container_name: &'a str,
    config: &'a ManifestContainerConfig,
    policy: &'a EffectiveContainerPolicy,
    service: &'a str,
    command: &'a [String],
    output_json: bool,
    alias_name: Option<&'a str>,
}

fn run_raw_exec(request: RawExecRequest<'_>) -> Result<String, RunnerError> {
    let RawExecRequest {
        repo_root,
        invocation_cwd,
        container_name,
        config,
        policy,
        service,
        command,
        output_json,
        alias_name,
    } = request;
    ensure_container_running(repo_root, policy, container_name)?;
    let workspace_identity = if service == policy.primary_service {
        policy
            .workspace_user
            .as_deref()
            .map(|user| (user, policy.workspace_home.as_deref()))
    } else {
        None
    };
    let args = build_raw_exec_args(
        repo_root,
        invocation_cwd,
        container_name,
        config,
        service,
        command,
        RawExecOptions {
            workspace_identity,
            allocate_tty: !output_json
                && std::io::stdin().is_terminal()
                && std::io::stdout().is_terminal(),
        },
    )?;
    let output = run_compose_exec(repo_root, policy, &args, output_json, "docker compose exec")?;
    render_exec_result(
        container_name,
        service,
        command,
        output,
        output_json,
        alias_name,
    )
}

enum RoutedTaskExecSurface<'a> {
    Named {
        container_name: &'a str,
    },
    ResolvedPolicy {
        policy: &'a EffectiveContainerPolicy,
        working_dir: &'a Path,
    },
}

fn run_routed_task_container_exec_variant(
    request: &RoutedTaskExecRequest<'_>,
    surface: RoutedTaskExecSurface<'_>,
    capture: bool,
) -> Result<Output, RunnerError> {
    let repo_root = request.repo_root;
    let invocation_cwd = request.invocation_cwd;
    match surface {
        RoutedTaskExecSurface::Named { container_name } => {
            let surface = resolve_running_named_exec_surface(repo_root, container_name)?;
            let mapped_cwd = map_host_cwd(
                repo_root,
                invocation_cwd,
                &surface.container_name,
                &surface.config,
            )?;
            run_routed_task_exec_internal_with_mapped_cwd(
                request,
                &surface.policy,
                &mapped_cwd,
                capture,
            )
        }
        RoutedTaskExecSurface::ResolvedPolicy {
            policy,
            working_dir,
        } => {
            let mapper =
                effigy_exec::CwdMapper::new(repo_root.to_path_buf(), working_dir.to_path_buf());
            let mapped_cwd = mapper
                .host_to_container(invocation_cwd)
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            run_routed_task_exec_internal_with_mapped_cwd(request, policy, &mapped_cwd, capture)
        }
    }
}

fn run_routed_task_exec_internal_with_mapped_cwd(
    request: &RoutedTaskExecRequest<'_>,
    policy: &EffectiveContainerPolicy,
    mapped_cwd: &str,
    capture: bool,
) -> Result<Output, RunnerError> {
    let RoutedTaskExecRequest {
        repo_root,
        selector,
        task_args,
        service,
        command,
        task_env,
        secret_env,
        ..
    } = request;
    let raw_command = vec!["sh".to_owned(), "-lc".to_owned(), (*command).to_owned()];
    if routed_task_exec_requires_workspace_effigy(policy, service) {
        ensure_workspace_effigy_available_for_policy(repo_root, policy, None)?;
    }
    let capabilities = transport::probe_container_capabilities(repo_root, policy, service)?;
    let selector_name = render_task_selector(selector);
    let strategy = determine_strategy(
        &capabilities,
        &selector_name,
        task_args,
        mapped_cwd,
        &raw_command,
    );
    if strategy_requires_workspace_effigy_install(&strategy) {
        ensure_workspace_effigy_available_for_policy(repo_root, policy, None)?;
    }
    let workspace_identity = if *service == policy.primary_service {
        policy
            .workspace_user
            .as_deref()
            .map(|user| (user, policy.workspace_home.as_deref()))
    } else {
        None
    };
    let args = build_routed_task_exec_args(
        &strategy,
        *task_env,
        *secret_env,
        workspace_identity,
        service,
        mapped_cwd,
    );

    run_compose_exec(repo_root, policy, &args, capture, "docker compose exec")
}

fn finish_routed_task_exec(command: &str, output: Output) -> Result<String, RunnerError> {
    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: command.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::new())
}

pub(super) fn strategy_requires_workspace_effigy_install(
    strategy: &effigy_exec::ExecStrategy,
) -> bool {
    matches!(strategy, effigy_exec::ExecStrategy::Handoff { .. })
}

pub(super) fn routed_task_exec_requires_workspace_effigy(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> bool {
    policy.workspace_user.is_some() && service == policy.primary_service
}

fn map_host_cwd(
    repo_root: &Path,
    invocation_cwd: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
) -> Result<String, RunnerError> {
    let working_dir = resolve_exec_working_dir(repo_root, container_name, config)?;
    let mapper = effigy_exec::CwdMapper::new(repo_root.to_path_buf(), working_dir);
    mapper
        .host_to_container(invocation_cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn render_exec_result(
    container_name: &str,
    service: &str,
    command: &[String],
    output: Output,
    output_json: bool,
    alias_name: Option<&str>,
) -> Result<String, RunnerError> {
    if output_json {
        return Ok(serde_json::json!({
            "schema": "effigy.exec.v1",
            "schema_version": 1,
            "ok": output.status.success(),
            "container": container_name,
            "service": service,
            "alias": alias_name,
            "command": command,
            "code": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        })
        .to_string());
    }

    if !output.status.success() {
        return Err(RunnerError::TaskCommandFailure {
            command: command.join(" "),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests;
