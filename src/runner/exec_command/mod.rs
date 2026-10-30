use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_builtin::BuiltinContainerSuiteTarget;
use effigy_cli::ExecArgs;
use effigy_containers::load_container_policy;
use effigy_containers::EffectiveContainerPolicy;
use effigy_core::shell::shell_quote;
use effigy_env::secret::SecretString;
use effigy_exec::detection::determine_strategy;
use effigy_manifest::ManifestContainerConfig;
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeActivationRoute};
use effigy_tasks::{render_task_selector, TaskSelector};

use super::command_context::resolve_active_command_context;
use super::container_runtime::inside_container_handoff;
use super::container_runtime_prep::{
    activate_container_runtime_for_task, build_runtime_activation_plan, ActivationRequest,
    ContainerTaskActivation,
};
use super::environment::emit_held_notice;
use super::error::RunnerError;
use super::runtime_session_context::{current_runtime_session_context, RuntimeSessionContext};
use super::task_engine::{PlainExit, RemoteTask, TaskRun};
use super::workspace_command::ensure_workspace_effigy_available_for_policy;
use surface::{
    build_alias_table, build_raw_exec_args, ensure_container_running, exec_alias_surface_absent,
    resolve_default_exec_surface, resolve_exec_working_dir, resolve_running_named_exec_surface,
    RawExecOptions, ResolvedExecSurface,
};

mod bounded;
mod surface;
mod transport;

pub(in crate::runner) use transport::{
    append_color_exec_env, copy_file_into_service, copy_file_into_service_bounded,
    probe_container_capabilities, run_command_capture_until, run_compose_exec,
    run_compose_exec_plan_with_credentials, run_compose_exec_plan_with_deadline,
    run_compose_exec_plan_with_options, run_managed_compose_exec_plan,
    run_verified_compose_exec_plan, runtime_backend_is_running_bounded, verified_target_exec_args,
};
#[cfg(not(test))]
pub(in crate::runner) use transport::{capture_command_until, CommandCaptureError};

pub(in crate::runner) use bounded::{
    BoundedCapture, BoundedCaptureFailure, BoundedCommandBudget, BoundedCommandPhase,
    BoundedObservedBytes, BoundedRefusalObservations, BoundedScopedCaptureFailure, BoundedStream,
    BoundedStreamRefusalObservation,
};
// Observation vocabulary and construction used by the private refusal
// accounting proofs; not part of the non-test runtime surface.
#[cfg(test)]
pub(in crate::runner) use bounded::{
    refusal_stream_observation, BoundedLeaderExitObservation, BoundedSpawnDisposition,
    BoundedStreamProgress, BoundedStreamRefusalState, StreamOutcome,
    BOUNDED_REFUSAL_OBSERVATIONS_VERSION,
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

/// Whether `alias_name` is an exec alias of the default exec surface. Reads
/// the surface declaration only: no activation, no probe.
pub(in crate::runner) fn exec_alias_declared(
    repo_root: &Path,
    alias_name: &str,
    extra_args: &[String],
) -> Result<bool, RunnerError> {
    let surface = match resolve_default_exec_surface(repo_root) {
        Ok(surface) => surface,
        Err(error) if exec_alias_surface_absent(&error) => return Ok(false),
        Err(error) => return Err(error),
    };
    let alias_table = build_alias_table(&surface.config)?;
    match alias_table.resolve_command(alias_name, extra_args) {
        Ok(_) => Ok(true),
        Err(effigy_exec::ExecError::AliasNotFound { .. }) => Ok(false),
        Err(error) => Err(RunnerError::task_invocation(error.to_string())),
    }
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
    // Inside the workspace container the alias command is already local:
    // run it directly without activating (and probing) the host runtime.
    if let Some(local) = run_handoff_local_raw_exec(HandoffLocalRawExec {
        repo_root,
        invocation_cwd,
        container_name: &surface.container_name,
        config: &surface.config,
        service: &alias.service,
        command: &alias.command,
        output_json,
        alias_name: Some(alias_name),
    }) {
        return local.map(Some);
    }
    let activation = activate_exec_surface(repo_root, &surface)?;

    let output = run_raw_exec(RawExecRequest {
        repo_root,
        invocation_cwd,
        container_name: &surface.container_name,
        config: &surface.config,
        policy: &activation.policy,
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

/// Run an `effigy exec` command where the caller already is when inside
/// the workspace container. Returns `None` on the host so host flows keep
/// their activation and runtime checks.
///
/// The container working directory resolves exactly as the compose path
/// would map it, but the command executes as a local process: no
/// `colima`/`nerdctl`/`docker` probe ever runs inside the container.
struct HandoffLocalRawExec<'a> {
    repo_root: &'a Path,
    invocation_cwd: &'a Path,
    container_name: &'a str,
    config: &'a ManifestContainerConfig,
    service: &'a str,
    command: &'a [String],
    output_json: bool,
    alias_name: Option<&'a str>,
}

fn run_handoff_local_raw_exec(
    request: HandoffLocalRawExec<'_>,
) -> Option<Result<String, RunnerError>> {
    let HandoffLocalRawExec {
        repo_root,
        invocation_cwd,
        container_name,
        config,
        service,
        command,
        output_json,
        alias_name,
    } = request;
    let resolved = super::container_runtime::resolve_handoff_local_execution(
        repo_root,
        Some(container_name),
        Some(service),
    )?;
    let mut execution = match resolved {
        Ok(execution) => execution,
        Err(error) => return Some(Err(error)),
    };
    let mapped_cwd = match surface::map_host_cwd(repo_root, invocation_cwd, container_name, config)
    {
        Ok(mapped) => mapped,
        Err(error) => return Some(Err(error)),
    };
    execution.working_dir = PathBuf::from(mapped_cwd);
    let output = match super::container_runtime::run_handoff_local_direct_command(
        &execution,
        command,
        output_json,
    ) {
        Ok(output) => output,
        Err(error) => return Some(Err(error)),
    };
    Some(render_exec_result(
        container_name,
        service,
        command,
        output,
        output_json,
        alias_name,
    ))
}

fn run_explicit_exec(
    repo_root: &Path,
    invocation_cwd: &Path,
    service_override: Option<&str>,
    command: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let surface = resolve_default_exec_surface(repo_root)?;
    let service = service_override.unwrap_or(surface.policy.primary_service.as_str());
    // Inside the workspace container the command is already local: run it
    // directly without activating (and probing) the host runtime.
    if let Some(local) = run_handoff_local_raw_exec(HandoffLocalRawExec {
        repo_root,
        invocation_cwd,
        container_name: &surface.container_name,
        config: &surface.config,
        service,
        command,
        output_json,
        alias_name: None,
    }) {
        return local;
    }
    let activation = activate_exec_surface(repo_root, &surface)?;
    let output = run_raw_exec(RawExecRequest {
        repo_root,
        invocation_cwd,
        container_name: &surface.container_name,
        config: &surface.config,
        policy: &activation.policy,
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
    if activation.started_held {
        emit_held_notice(&surface.container_name);
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

pub(in crate::runner) enum RoutedTaskExecSurface<'a> {
    Named {
        container_name: &'a str,
        policy: &'a EffectiveContainerPolicy,
    },
    ResolvedPolicy {
        policy: &'a EffectiveContainerPolicy,
        working_dir: &'a Path,
    },
}

/// A routed task's command in its service container, under the run's
/// remote custody (052 §5.10). Text relays output as it arrives; JSON
/// captures it. Both run the same way (K11).
pub(in crate::runner) fn run_routed_task_remote(
    request: &RoutedTaskExecRequest<'_>,
    surface: RoutedTaskExecSurface<'_>,
    run: &mut TaskRun,
    capture: bool,
) -> Result<PlainExit, RunnerError> {
    let repo_root = request.repo_root;
    let (policy, mapped_cwd) = match surface {
        RoutedTaskExecSurface::Named {
            container_name,
            policy,
        } => {
            let surface = resolve_running_named_exec_surface(repo_root, container_name)?;
            let mapped_cwd = map_host_cwd(
                repo_root,
                request.invocation_cwd,
                &surface.container_name,
                &surface.config,
            )?;
            (policy, mapped_cwd)
        }
        RoutedTaskExecSurface::ResolvedPolicy {
            policy,
            working_dir,
        } => {
            let mapper =
                effigy_exec::CwdMapper::new(repo_root.to_path_buf(), working_dir.to_path_buf());
            let mapped_cwd = mapper
                .host_to_container(request.invocation_cwd)
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            (policy, mapped_cwd)
        }
    };
    let service = request.service;
    let raw_command = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        request.command.to_owned(),
    ];
    if routed_task_exec_requires_workspace_effigy(policy, service) {
        ensure_workspace_effigy_available_for_policy(repo_root, policy, None)?;
    }
    let capabilities = transport::probe_container_capabilities(repo_root, policy, service)?;
    let selector_name = render_task_selector(request.selector);
    let strategy = determine_strategy(
        &capabilities,
        &selector_name,
        request.task_args,
        &mapped_cwd,
        &raw_command,
    );
    if strategy_requires_workspace_effigy_install(&strategy) {
        ensure_workspace_effigy_available_for_policy(repo_root, policy, None)?;
    }
    let workspace_identity = workspace_identity(policy, service);
    let exec = transport::routed_task_exec(
        &strategy,
        request.task_env,
        request.secret_env,
        workspace_identity,
        &mapped_cwd,
        // Colima reports no backend terminal (the kernel frames on stderr),
        // so only Docker workloads get terminal colour settings.
        crate::runner::task_engine::remote_tty(capture)
            && crate::runner::environment::backend(repo_root, policy).kind
                != effigy_containers::environment::BackendKind::Colima,
    );
    run_in_service(
        repo_root,
        policy,
        service,
        exec,
        run,
        capture,
        request.command,
    )
}

/// A built-in `exec` step of a sequence: the same surface, activation and
/// working-directory mapping as `effigy exec`, under the step's remote
/// custody. The activation's hold lasts until the workload has ended.
pub(in crate::runner) fn run_exec_step(
    args: &ExecArgs,
    run: &mut TaskRun,
    capture: bool,
) -> Result<PlainExit, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let repo_root = context.resolved.resolved_root.as_path();
    let invocation_cwd = context.invocation_cwd.as_path();
    if args.command.is_empty() {
        return Err(RunnerError::task_invocation(
            "`effigy exec` requires a command to run",
        ));
    }
    let surface = resolve_default_exec_surface(repo_root)?;
    let service = args
        .service
        .as_deref()
        .unwrap_or(surface.policy.primary_service.as_str());
    let activation = activate_exec_surface(repo_root, &surface)?;
    let policy = &activation.policy;
    ensure_container_running(repo_root, policy, &surface.container_name)?;
    let mut env = Vec::new();
    let user = workspace_identity(policy, service).map(|(user, home)| {
        if let Some(home) = home {
            env.push((
                "HOME".to_owned(),
                crate::runner::task_engine::EnvOverride::Plain(home.to_owned()),
            ));
        }
        user.to_owned()
    });
    let exec = transport::RoutedExec {
        argv: args.command.clone(),
        env,
        user,
        workdir: map_host_cwd(
            repo_root,
            invocation_cwd,
            &surface.container_name,
            &surface.config,
        )?,
    };
    let label = args.command.join(" ");
    let exit = run_in_service(repo_root, policy, service, exec, run, capture, &label);
    maybe_emit_exec_activation_notice(&surface, activation);
    exit
}

fn workspace_identity<'a>(
    policy: &'a EffectiveContainerPolicy,
    service: &str,
) -> Option<(&'a str, Option<&'a str>)> {
    if service == policy.primary_service {
        policy
            .workspace_user
            .as_deref()
            .map(|user| (user, policy.workspace_home.as_deref()))
    } else {
        None
    }
}

fn run_in_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    exec: transport::RoutedExec,
    run: &mut TaskRun,
    capture: bool,
    label: &str,
) -> Result<PlainExit, RunnerError> {
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        repo_root, policy, service, None,
    )?;
    crate::runner::task_engine::run_remote(
        run,
        policy,
        RemoteTask {
            repo_root,
            container_id: binding.id(),
            argv: exec.argv,
            env: exec.env,
            user: exec.user,
            workdir: Some(exec.workdir),
            capture,
            timeout_ms: None,
            label: label.to_owned(),
        },
    )
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

/// Resolve the container suite target for one builtin test target.
///
/// Returns `None` when suites must stay on the local process surface (inside
/// an effigy container handoff the local environment already is the
/// container). The service resolves from the owning manifest's container
/// policy, the same policy container task execution uses.
pub(in crate::runner) fn resolve_container_suite_target(
    target_root: &Path,
    container: &str,
) -> Result<Option<BuiltinContainerSuiteTarget>, RunnerError> {
    if inside_container_handoff() {
        return Ok(None);
    }
    let policy = load_container_policy(target_root, Some(container))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(Some(BuiltinContainerSuiteTarget {
        container: container.to_owned(),
        service: policy.primary_service.clone(),
        root: target_root.to_path_buf(),
    }))
}

/// Resolve and activate a container suite target so suite execs can assume
/// the declared runtime is up. Shares the standard task activation path.
pub(in crate::runner) fn prepare_container_suite_target(
    target_root: &Path,
    container: &str,
) -> Result<Option<BuiltinContainerSuiteTarget>, RunnerError> {
    let target = resolve_container_suite_target(target_root, container)?;
    if target.is_some() {
        crate::runner::container_runtime_prep::activate_routed_container_runtime(
            target_root,
            container,
        )?;
    }
    Ok(target)
}

/// Render one builtin test suite's lifecycle command as a container exec
/// command line against the resolved suite target.
///
/// The rendered line uses the same compose invocation and exec argument
/// shape as routed task execution, so suite commands observe the same
/// working-dir mapping and workspace identity as tasks.
pub(in crate::runner) fn render_container_suite_command(
    target: &BuiltinContainerSuiteTarget,
    suite_command: &str,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(&target.root, Some(&target.container))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let working_dir =
        effigy_containers::load_container_exec_working_dir(&target.root, Some(&target.container))
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mapped_cwd = working_dir.to_string_lossy().into_owned();
    let raw_command = vec!["sh".to_owned(), "-lc".to_owned(), suite_command.to_owned()];
    let workspace_identity = if target.service == policy.primary_service {
        policy
            .workspace_user
            .as_deref()
            .map(|user| (user, policy.workspace_home.as_deref()))
    } else {
        None
    };
    let strategy = effigy_exec::ExecStrategy::RawExec {
        working_dir: mapped_cwd.clone(),
        command: raw_command,
    };
    let exec_args = transport::build_routed_task_exec_args(
        &strategy,
        None,
        None,
        workspace_identity,
        &target.service,
        &mapped_cwd,
    );
    let binding = crate::runner::container_runtime_prep::verify_service_binding(
        &target.root,
        &policy,
        &target.service,
        None,
    )?;
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        &target.root,
        &policy,
        exec_args,
        effigy_containers::ContainerAction::Exec,
        "container suite exec",
    )
    .map_err(RunnerError::from)?;
    let (program, compose_args) =
        transport::verified_native_exec_invocation(&policy, &plan, &binding, false, None)?;
    let mut rendered = format!("cd {}", shell_quote(&target.root.display().to_string()));
    rendered.push_str(" && ");
    rendered.push_str(&shell_quote(&program.to_string_lossy()));
    for arg in compose_args {
        rendered.push(' ');
        rendered.push_str(&shell_quote(&arg.to_string_lossy()));
    }
    Ok(rendered)
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
