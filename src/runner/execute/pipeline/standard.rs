use std::collections::BTreeSet;
use std::path::Path;

use super::super::super::container_command::is_primary_service_running;
use super::super::super::exec_command::{
    run_routed_task_remote, RoutedTaskExecRequest, RoutedTaskExecSurface,
};
use super::super::binding::{
    resolve_execution_binding_resolution, ContainerExecutionBinding, ExecutionBindingResolution,
};
use super::super::context::ExecutionTaskContext;
use super::super::host_alias::{secret_env_slice, HostAliasResolver};
use super::super::planning::ExecutionPreflight;
use super::super::routing::{
    route_standard_task_execution, routed_container_target, routed_not_running_container,
    RoutedTaskExecution,
};
use super::super::runtime_inputs::{effective_task_binding_inputs, execution_scope_root};
use super::{super::process_run, command};
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, build_runtime_activation_plan, ActivationRequest,
};
use crate::runner::environment::emit_held_notice;
use crate::runner::error::RunnerError;
use crate::runner::execute::nested;
use crate::runner::execute::render;
use crate::runner::execute::workspace_seeded::{
    inside_container_handoff, run_workspace_seeded_task_session,
};
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use crate::runner::runtime_session_context::{
    current_runtime_session_context, LeaseRefreshPolicy, RuntimeSessionContext,
};
use crate::runner::task_engine::{
    resolve_lock_wait_ms_or, RunExec, RunKey, TaskRun, TaskRunRequest, TaskTarget,
};
use crate::runner::task_secrets::{redact_task_secret_values, resolve_standard_task_secrets};
use effigy_containers::{load_container_policy, ContainerAction, ContainerComposeInvocationPlan};
use effigy_env::resolver::ResolvedEnv;
use effigy_env::schema_support::{
    resolve_catalog_env_schema_with as shared_resolve_env_schema, SchemaSupportConfig,
    SchemaSupportError,
};
use effigy_env::secret::SecretString;
use effigy_execution::{ExecutionBindingInput, ExecutionSelectionPlan};
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_manifest::{ManifestManagedRun, ManifestTaskRunIn, TaskSelection};
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeActivationRoute};

#[path = "standard/reserved.rs"]
mod reserved;
pub(in crate::runner) use reserved::{run_reserved_shell, validate_reserved_shell};

pub(in crate::runner) fn run_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &ExecutionSelectionPlan,
    run_key: Option<&RunKey>,
    tree: Option<super::super::sequence_run::ResolvedSteps>,
) -> Result<String, RunnerError> {
    if selection.task.cache.is_some() {
        return Err(retired_task_cache_error(&preflight.selector.task_name));
    }
    let target = TaskTarget::for_selection(
        &preflight.resolved.resolved_root,
        &preflight.selector.task_name,
        selection,
    )
    .with_arguments(&preflight.runtime_args_exec.passthrough);
    let inherited_managed_run = std::env::var(effigy_managed::RUN_TOKEN_ENV)
        .ok()
        .filter(|token| !token.is_empty());
    let env_schema_catalog = effigy_manifest::env_schema_declaring_catalog(
        &preflight.catalogs,
        &selection.catalog.catalog_root,
    );
    let env_schema_root = env_schema_catalog
        .map_or(selection.catalog.catalog_root.as_path(), |catalog| {
            catalog.catalog_root.as_path()
        });
    let env_schema_config =
        env_schema_catalog.and_then(|catalog| catalog.manifest.env_schema.as_ref());
    let env_schema_override = preflight.runtime_args_raw.env_schema_override.as_deref();
    let tree = match tree {
        Some(tree) => tree,
        None => super::super::sequence_run::resolve_tree(preflight, selection)?,
    };
    let memory = run_key.is_none()
        && inherited_managed_run.is_none()
        && !inside_container_handoff()
        && !crate::runner::program_fallback::activates_here()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        && tree.foreground;
    let begin = if memory {
        TaskRun::begin_foreground
    } else {
        TaskRun::begin
    };
    let mut run = begin(TaskRunRequest {
        target: &target,
        lock_wait_ms: resolve_lock_wait_ms_or(
            preflight.runtime_args_raw.lock_wait_ms,
            target.default_lock_wait_ms(),
        )?,
        // Host commands, env-schema commands and container workloads all
        // run under the run's custody; every route plans its sentinel.
        custody: true,
        // Only a host-placed task gets the configured per-run scratch, and a
        // task may opt out of it.
        scratch: runs_plain_host_command(preflight, selection) && selection.task.uses_scratch(),
        managed_run_token: inherited_managed_run.as_deref(),
        args: &preflight.runtime_args_exec.passthrough,
        key: run_key,
        admission: selection.task.admission,
        workspace_admission: None,
        cwd: Some(&preflight.invocation_cwd),
    })?;
    let mut workload = None;
    // Admitted commits before env-schema commands run (K3): each `exec()`
    // value is an effect of this run, and a second spelling waits on the
    // gate instead of running them too.
    let result = (|| {
        // The task's own vault secrets resolve before the run's lexical scope:
        // a first local-dev unlock writes the machine vault, and the kernel
        // refuses a durable effect while a foreground parent scope is live.
        let task_secret_pairs = resolve_standard_task_secrets(preflight, selection)?;
        let _parent = run.enter();
        let env_schema_resolved = {
            let exec = RunExec::new(&mut run);
            resolve_env_schema_if_present(
                env_schema_root,
                env_schema_override,
                env_schema_config,
                Some(&exec),
            )?
        };
        let built = command::build_task_command(preflight, selection, &env_schema_resolved)?;
        let plain_env_keys = built.plain_env_keys.clone();
        let context = ExecutionTaskContext::new(preflight, selection, built.command);
        run_standard_task_inner(
            preflight,
            selection,
            selection_plan,
            &context,
            &env_schema_resolved,
            Some(&task_secret_pairs),
            built.host_aliases,
            &plain_env_keys,
            &mut run,
            &mut workload,
            tree,
        )
    })();
    let mut workload = workload.unwrap_or_else(|| workload_for_result(&result));
    // A signal consumed before the command ran (for example during an
    // env-schema `exec()`) still cancels the run: 128 + n, recorded so.
    let result = match crate::runner::task_engine::interrupted_by() {
        Some(signal) if result.is_err() => {
            workload = crate::runner::task_engine::workload_for_exit(128 + signal);
            if result
                .as_ref()
                .err()
                .and_then(RunnerError::task_exit_status)
                == Some(128 + signal)
            {
                result
            } else {
                Err(RunnerError::TaskCommandFailure {
                    command: preflight.selector.task_name.clone(),
                    code: Some(128 + signal),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        _ => result,
    };
    // Observability never changes the task's outcome or exit code (K10).
    for warning in run.close(workload) {
        let _ = effigy_core::output_cap::write(
            effigy_core::output_cap::Stream::Stderr,
            format!("warning: {}: {warning}\n", target.label).as_bytes(),
        );
    }
    result
}

/// The run's recorded outcome when the route did not report one itself.
fn workload_for_result(result: &Result<String, RunnerError>) -> WorkloadOutcome {
    match result {
        Ok(_) => WorkloadOutcome::Succeeded,
        Err(error) => error.workload_outcome(),
    }
}

/// Every run that may reach the plain-command route plans its sentinel
/// before admission, so custody (K6) exists before any spawn. Only a task
/// the host routes into a container never spawns locally; inside a
/// container handoff every task runs locally.
pub(in crate::runner) fn runs_plain_host_command(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> bool {
    if inside_container_handoff() {
        return true;
    }
    checked_host_binding(preflight, selection, &preflight.selector.task_name).unwrap_or(false)
}

pub(in crate::runner::execute) fn checked_host_binding(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    task_name: &str,
) -> Result<bool, RunnerError> {
    let (default_run_in, systems, containers) =
        effective_task_binding_inputs(&preflight.invocation_cwd, &preflight.catalogs, selection);
    let binding = resolve_execution_binding_resolution(
        default_run_in,
        systems.as_ref(),
        containers.as_ref(),
        task_name,
        selection.task,
        "run-class planning",
    )?;
    Ok(matches!(
        binding.binding(),
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host
    ))
}

fn retired_task_cache_error(task_name: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "task `{task_name}` declares `cache`, which was removed in v0.15: Effigy no longer skips \
         tasks from a fingerprint. Delete the `cache` table and let the tool's own cache \
         (cargo, turbo, vite) decide; the retired `.effigy/cache/task-cache-v1.json` can be deleted"
    ))
}

// Pre-existing 8-argument signature: this shape predates the feedback
// foundation and the current toolchain's `too_many_arguments` denies it. The
// split is its own task; the allow keeps the shared `-D warnings` lint group
// meaningful for everything else instead of narrowing the group.
#[allow(clippy::too_many_arguments)]
fn run_standard_task_inner(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &ExecutionSelectionPlan,
    context: &ExecutionTaskContext<'_>,
    env_schema_resolved: &Option<ResolvedEnv>,
    resolved_task_secrets: Option<&[(String, SecretString)]>,
    mut host_aliases: Option<HostAliasResolver>,
    plain_env_keys: &BTreeSet<String>,
    run: &mut TaskRun,
    workload: &mut Option<WorkloadOutcome>,
    tree: super::super::sequence_run::ResolvedSteps,
) -> Result<String, RunnerError> {
    let env_schema_secret_pairs = env_schema_resolved
        .as_ref()
        .map(|resolved| resolved.secret_env())
        .unwrap_or_default();
    let task_secret_pairs = match resolved_task_secrets {
        Some(pairs) => pairs.to_vec(),
        None => resolve_standard_task_secrets(preflight, selection)?,
    };
    let mut secret_pairs = env_schema_secret_pairs;
    for (key, value) in &task_secret_pairs {
        secret_pairs.push((key.as_str(), value));
    }
    let secret_ref = (!secret_pairs.is_empty()).then_some(secret_pairs.as_slice());

    if let Some(output) = nested::maybe_run_in_process_sequence(
        preflight,
        selection,
        context,
        env_schema_resolved,
        secret_ref,
        (run, workload),
        tree,
    )? {
        return Ok(output);
    }

    let (default_run_in, systems, containers) =
        effective_task_binding_inputs(&preflight.invocation_cwd, &preflight.catalogs, selection);
    let scope_root =
        execution_scope_root(&preflight.invocation_cwd, &preflight.catalogs, selection);

    let binding_resolution = resolve_execution_binding_resolution(
        default_run_in,
        systems.as_ref(),
        containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "standard task execution",
    )?;
    let _binding_plan = binding_resolution.plan(ExecutionBindingInput::new(
        selection_plan.clone(),
        "standard task execution",
    ));
    let container_binding = binding_resolution.binding();
    if let ContainerExecutionBinding::Inline { .. } = container_binding {
        let output = run_inline_workspace_standard_task(
            preflight,
            selection,
            context,
            secret_ref,
            &binding_resolution,
            run,
            workload,
        )?;
        return Ok(output);
    }

    if matches!(
        container_binding,
        ContainerExecutionBinding::Container { .. }
    ) {
        effigy_core::worktree_scope::prepare_checkout_token(scope_root).map_err(|error| {
            RunnerError::task_invocation(format!("cannot record the checkout identity: {error}"))
        })?;
    }
    let routed = route_with_running_check(
        scope_root,
        preflight,
        selection,
        default_run_in,
        systems.as_ref(),
        containers.as_ref(),
    )?;

    let mut task_activation = None;
    let routed = if let Some(container_name) = routed_not_running_container(&routed.decision) {
        task_activation = Some(
            crate::runner::container_runtime_prep::activate_routed_container_runtime(
                scope_root,
                container_name,
            )?,
        );
        let rerouted = route_with_running_check(
            scope_root,
            preflight,
            selection,
            default_run_in,
            systems.as_ref(),
            containers.as_ref(),
        )?;
        if rerouted.decision.is_not_running() {
            return Err(RunnerError::task_invocation(format!(
                "task `{}` requires container `{}` but it is still not running after auto-up",
                preflight.selector.task_name, container_name
            )));
        }
        rerouted
    } else {
        if let Some((container_name, _)) = routed_container_target(&routed.decision) {
            task_activation = Some(
                crate::runner::container_runtime_prep::activate_routed_container_runtime(
                    scope_root,
                    container_name,
                )?,
            );
        }
        routed
    };

    if should_stay_in_workspace_shell(preflight.output_json, selection.task, container_binding) {
        let output = run_workspace_seeded_task_session(
            &selection.catalog.catalog_root,
            container_binding,
            preflight.runtime_args_raw.repo_override.clone(),
            &preflight.selector.task_name,
            &preflight.runtime_args_exec.passthrough,
            Some(current_runtime_session_context().public_workspace_cleanup),
        )?;
        return Ok(output);
    }

    if let Some((container, service)) = routed_container_target(&routed.decision) {
        let activated_policy = &task_activation
            .as_ref()
            .ok_or_else(|| {
                RunnerError::task_invocation("container route missing its activation snapshot")
            })?
            .policy;
        let request = RoutedTaskExecRequest {
            repo_root: scope_root,
            invocation_cwd: preflight.task_execution_root(&selection.catalog.catalog_root),
            selector: &preflight.selector,
            task_args: &preflight.runtime_args_exec.passthrough,
            service,
            command: context.command(),
            task_env: Some(&context.selection.task.env),
            secret_env: secret_ref,
        };
        let exit = run_routed_task_remote(
            &request,
            RoutedTaskExecSurface::Named {
                container_name: container,
                policy: activated_policy,
            },
            run,
            preflight.output_json,
        )?;
        if exit.workload == WorkloadOutcome::Succeeded
            && task_activation
                .as_ref()
                .is_some_and(|activation| activation.started_held)
        {
            emit_held_notice(container);
        }
        return finish_remote(preflight, context, secret_ref, exit, workload);
    }

    // A host run in a runtime-scoped checkout resolves the canonical declared
    // backing-service alias in everything it delivers: the secret values and
    // the values it would otherwise inherit unchanged (a `DATABASE_URL` in the
    // caller's environment names the declared alias just as a task's own `env`
    // does). It refuses before the child starts when this checkout has no
    // published backing service, and the one resolution lives with the alias
    // owner (contract 005, effigy#431 review).
    let (resolved_secret_env, inherited_env) = match host_aliases.as_mut() {
        Some(resolver) => {
            let (secret_env, inherited) = resolver.resolve_delivered_env(
                plain_env_keys,
                secret_ref.unwrap_or_default(),
                std::iter::empty(),
                true,
            )?;
            (Some(secret_env), inherited)
        }
        None => (None, Vec::new()),
    };
    let resolved_secret_refs = resolved_secret_env
        .as_deref()
        .map(secret_env_slice)
        .unwrap_or_default();
    let secret_ref = if resolved_secret_env.is_some() {
        Some(resolved_secret_refs.as_slice())
    } else {
        secret_ref
    };

    process_run::run_task_process(
        preflight.output_json,
        preflight.runtime_args_raw.verbose_root,
        context,
        secret_ref,
        &inherited_env,
        run,
        workload,
    )
}

fn route_with_running_check(
    scope_root: &Path,
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    default_run_in: Option<ManifestTaskRunIn>,
    systems: Option<&effigy_manifest::ManifestSystemsConfig>,
    containers: Option<&effigy_manifest::ManifestContainersConfig>,
) -> Result<RoutedTaskExecution, RunnerError> {
    route_standard_task_execution(
        &preflight.selector.task_name,
        default_run_in,
        selection.task,
        systems,
        containers,
        |container_name| {
            let policy = load_container_policy(scope_root, Some(container_name))?;
            is_primary_service_running(scope_root, &policy)
        },
    )
}

fn should_stay_in_workspace_shell(
    output_json: bool,
    task: &effigy_manifest::ManifestTask,
    container_binding: &ContainerExecutionBinding,
) -> bool {
    should_stay_in_workspace_shell_in(
        inside_container_handoff(),
        output_json,
        task,
        container_binding,
    )
}

/// Testable form of [`should_stay_in_workspace_shell`] that takes the
/// container-handoff env fact explicitly instead of reading the process
/// environment, so parallel tests cannot overwrite one another's env.
fn should_stay_in_workspace_shell_in(
    inside_container_handoff: bool,
    output_json: bool,
    task: &effigy_manifest::ManifestTask,
    container_binding: &ContainerExecutionBinding,
) -> bool {
    if output_json
        || inside_container_handoff
        || !task.stay_in_shell.unwrap_or(false)
        || task.workspace.is_none()
        || task.run.is_none()
    {
        return false;
    }

    matches!(
        container_binding,
        ContainerExecutionBinding::Container { .. }
    )
}

fn run_inline_workspace_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    context: &ExecutionTaskContext<'_>,
    secret_ref: Option<&[(&str, &SecretString)]>,
    binding_resolution: &ExecutionBindingResolution,
    run: &mut TaskRun,
    workload: &mut Option<WorkloadOutcome>,
) -> Result<String, RunnerError> {
    let repo_root = &selection.catalog.catalog_root;
    let policy = binding_resolution
        .effective_policy(repo_root)?
        .ok_or_else(|| RunnerError::task_invocation("missing inline workspace container policy"))?;
    let working_dir = binding_resolution
        .exec_working_dir(repo_root)?
        .ok_or_else(|| RunnerError::task_invocation("missing inline workspace exec working dir"))?;
    let activation = activate_inline_workspace_container_runtime(repo_root, &policy)?;
    let policy = activation.policy;
    let teardown_plan = inline_workspace_teardown_plan(repo_root, &policy);
    let request = RoutedTaskExecRequest {
        repo_root,
        invocation_cwd: &preflight.invocation_cwd,
        selector: &preflight.selector,
        task_args: &preflight.runtime_args_exec.passthrough,
        service: policy.primary_service.as_str(),
        command: context.command(),
        task_env: Some(&context.selection.task.env),
        secret_env: secret_ref,
    };
    let exit = run_routed_task_remote(
        &request,
        RoutedTaskExecSurface::ResolvedPolicy {
            policy: &policy,
            working_dir: &working_dir,
        },
        run,
        preflight.output_json,
    );
    teardown_inline_workspace_container(teardown_plan.as_ref());
    finish_remote(preflight, context, secret_ref, exit?, workload)
}

/// The route's result from a remote workload's exit: JSON renders the
/// captured streams, text has already relayed them.
fn finish_remote(
    preflight: &ExecutionPreflight,
    context: &ExecutionTaskContext<'_>,
    secret_ref: Option<&[(&str, &SecretString)]>,
    exit: crate::runner::task_engine::PlainExit,
    workload: &mut Option<WorkloadOutcome>,
) -> Result<String, RunnerError> {
    *workload = Some(exit.workload.clone());
    let succeeded = exit.workload == WorkloadOutcome::Succeeded;
    if preflight.output_json {
        let stdout = redact_task_secret_values(&String::from_utf8_lossy(&exit.stdout), secret_ref);
        let stderr = redact_task_secret_values(&String::from_utf8_lossy(&exit.stderr), secret_ref);
        let rendered = render::render_task_command_json(
            &preflight.selector,
            context.repo_for_task(),
            context.command(),
            Some(exit.code),
            &stdout,
            &stderr,
        )?;
        return if succeeded {
            Ok(rendered)
        } else {
            Err(RunnerError::CommandJsonFailure { rendered })
        };
    }
    if !succeeded {
        return Err(RunnerError::TaskCommandFailure {
            command: context.command().to_owned(),
            code: Some(exit.code),
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    if preflight.runtime_args_raw.verbose_root {
        return Ok(context.render_resolution_trace());
    }
    Ok(String::new())
}

fn inline_workspace_teardown_plan(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Option<ContainerComposeInvocationPlan> {
    effigy_runtime::container_manager::compose_invocation_plan(
        repo_root,
        policy,
        ["down", "--remove-orphans"],
        ContainerAction::Shutdown,
        "docker compose down",
    )
    .ok()
}

/// Tear down the container this task activated. The teardown is best
/// effort; its result never changes the task's outcome.
fn teardown_inline_workspace_container(plan: Option<&ContainerComposeInvocationPlan>) {
    if let Some(plan) = plan {
        let _ = effigy_runtime::signals::run_compose_plan_capture(plan);
    }
}

fn standard_runtime_activation_plan(
    repo_root: &Path,
    policy_name: &str,
    container_name: Option<String>,
    session_context: RuntimeSessionContext,
) -> RuntimeActivationPlan {
    build_runtime_activation_plan(
        repo_root,
        policy_name,
        container_name.as_deref(),
        Some(repo_root.to_path_buf()),
        RuntimeActivationRoute::Task,
        session_context,
    )
}

fn activate_inline_workspace_container_runtime(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<crate::runner::container_runtime_prep::ContainerTaskActivation, RunnerError> {
    activate_inline_workspace_container_runtime_with(
        repo_root,
        policy,
        |repo_root, policy, request, _plan| {
            activate_container_runtime_for_task(repo_root, policy, request)
        },
    )
}

fn activate_inline_workspace_container_runtime_with(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    activate: impl FnOnce(
        &Path,
        &effigy_containers::EffectiveContainerPolicy,
        ActivationRequest<'_>,
        &RuntimeActivationPlan,
    ) -> Result<
        crate::runner::container_runtime_prep::ContainerTaskActivation,
        RunnerError,
    >,
) -> Result<crate::runner::container_runtime_prep::ContainerTaskActivation, RunnerError> {
    let session_context = RuntimeSessionContext {
        lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
        ..current_runtime_session_context()
    };
    let plan = standard_runtime_activation_plan(
        repo_root,
        policy.name.as_str(),
        Some(policy.name.clone()),
        session_context,
    );
    activate(
        repo_root,
        policy,
        ActivationRequest {
            container_name: plan.request.container_name.as_deref(),
            repo_override: plan.request.repo_override.clone(),
            route: plan.route,
            session_context,
            reconcile_shared_services:
                crate::runner::container_runtime_prep::reconcile_shared_services_for(
                    policy,
                    session_context,
                ),
        },
        &plan,
    )
}

pub(in crate::runner) fn resolve_env_schema_if_present(
    catalog_root: &Path,
    runtime_override: Option<&Path>,
    config: Option<&ManifestEnvSchemaConfig>,
    exec: Option<&dyn effigy_env::ExecRunner>,
) -> Result<Option<ResolvedEnv>, RunnerError> {
    shared_resolve_env_schema(catalog_root, schema_config(config), runtime_override, exec)
        .map_err(map_schema_support_error)
}

fn schema_config(config: Option<&ManifestEnvSchemaConfig>) -> SchemaSupportConfig<'_> {
    SchemaSupportConfig {
        config_schema: config.and_then(|c| c.schema.as_deref()),
        config_enabled: config.and_then(|c| c.enabled),
        config_exec_timeout_secs: config.and_then(|c| c.exec_timeout),
    }
}

fn map_schema_support_error(error: SchemaSupportError) -> RunnerError {
    match error {
        SchemaSupportError::InvalidConfig(message) => RunnerError::task_invocation(message),
        SchemaSupportError::SchemaFileMissing(path) => {
            RunnerError::task_invocation(format!("env schema file not found: {}", path.display()))
        }
        SchemaSupportError::DotenvRead { path, error } => {
            RunnerError::task_invocation(format!("failed to read {}: {error}", path.display()))
        }
        SchemaSupportError::Schema(error) => RunnerError::EnvSchema(error),
    }
}

/// One legacy container command, selected by the typed sequence engine.
pub(in crate::runner) fn run_sequence_container_command(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    command_text: &str,
    run: &mut TaskRun,
) -> Result<String, RunnerError> {
    let mut task = selection.task.clone();
    task.run = Some(ManifestManagedRun::Command(command_text.to_owned()));
    task.env
        .extend(crate::runner::task_engine::spawn_plain_env());
    let selected = TaskSelection {
        task: &task,
        catalog: selection.catalog,
        mode: selection.mode,
        evidence: selection.evidence.clone(),
        surface: selection.surface,
    };
    let built = command::build_task_command(preflight, &selected, &None)?;
    let plain_env_keys = built.plain_env_keys.clone();
    let context = ExecutionTaskContext::new(preflight, &selected, built.command);
    let plan = super::super::selection::build_execution_selection_plan(preflight, &selected);
    let tree = super::super::sequence_run::resolve_tree(preflight, &selected)?;
    run_standard_task_inner(
        preflight,
        &selected,
        &plan,
        &context,
        &None,
        None,
        built.host_aliases,
        &plain_env_keys,
        run,
        &mut None,
        tree,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        activate_inline_workspace_container_runtime_with, should_stay_in_workspace_shell_in,
        ContainerExecutionBinding,
    };
    use crate::runner::container_runtime_prep::activate_routed_container_runtime_with;
    use crate::runner::container_runtime_prep::ContainerTaskActivation;
    use crate::runner::runtime_session_context::LeaseRefreshPolicy;
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{ManifestManagedRun, ManifestTask, ManifestTaskRunIn};
    use effigy_runtime_plan::RuntimeLeasePolicy;
    use std::path::{Path, PathBuf};

    fn stay_in_shell_task() -> ManifestTask {
        ManifestTask {
            workspace: Some("app".to_owned()),
            stay_in_shell: Some(true),
            run_in: Some(ManifestTaskRunIn::Container),
            run: Some(ManifestManagedRun::Command("printf seed".to_owned())),
            ..Default::default()
        }
    }

    #[test]
    fn stay_in_shell_requires_explicit_task_opt_in() {
        assert!(should_stay_in_workspace_shell_in(
            false,
            false,
            &stay_in_shell_task(),
            &ContainerExecutionBinding::Container {
                name: Some("web".to_owned()),
                workspace: None,
            },
        ));
    }

    #[test]
    fn stay_in_shell_is_disabled_inside_container_handoff() {
        assert!(!should_stay_in_workspace_shell_in(
            true,
            false,
            &stay_in_shell_task(),
            &ContainerExecutionBinding::Container {
                name: Some("web".to_owned()),
                workspace: None,
            },
        ));
    }

    #[test]
    fn routed_container_activation_uses_target_repo_root_as_repo_override() {
        let repo_root = Path::new("/tmp/demo-repo");
        let mut activation_call = None;

        let activation = activate_routed_container_runtime_with(
            repo_root,
            "web",
            |_repo_root, _container_name| {
                Ok(EffectiveContainerPolicy {
                    render: None,
                    repo_root: std::path::PathBuf::from("/tmp"),
                    name: "web".to_owned(),
                    driver: effigy_manifest::ManifestContainerDriver::Colima,
                    startup: effigy_manifest::ManifestContainerStartup::Detached,
                    profile: "effigy".to_owned(),
                    compose_source: EffectiveComposeSource::Direct,
                    compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
                    managed_volumes: vec![],
                    borrowed_volumes: Vec::new(),
                    data_peers: Vec::new(),
                    data_from: None,
                    shared_services: vec![],
                    project_name: "demo-web".to_owned(),
                    primary_service: "app".to_owned(),
                    dns_domain: None,
                    dns_base_domain: None,
                    runtime_host_key: None,
                    dns_tls: false,
                    dns_port: None,
                    dns_routes: vec![],
                    service_aliases: vec![],
                    declared_ports: vec![],
                    ports_declared_explicitly: false,
                    declared_mounts: vec![],
                    declared_media_mounts: vec![],
                    workspace_source: effigy_containers::artifact_source::WorkspaceSource::Checkout,
                    pull_production_hook: None,
                    health_check: None,
                    health_timeout_secs: 60,
                    secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
                    secret_runtime_dir: None,
                    source_secret_runtime_for_deferrals: false,
                    workspace_user: None,
                    workspace_home: None,
                    on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
                    shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
                    detach_timeout_secs: 10,
                    host_processes: Vec::new(),
                })
            },
            |repo_root, policy, request, plan| {
                activation_call = Some((
                    repo_root.to_path_buf(),
                    request.container_name.map(str::to_owned),
                    request.repo_override,
                    request.session_context.lease_refresh_policy,
                    plan.request.container_name.clone(),
                    plan.request.repo_override.clone(),
                    plan.lease.policy,
                ));
                Ok(ContainerTaskActivation {
                    policy: policy.clone(),
                    system_was_running: false,
                    started_held: true,
                    hold: None,
                })
            },
        )
        .expect("activate routed runtime");

        assert_eq!(
            activation_call,
            Some((
                repo_root.to_path_buf(),
                Some("web".to_owned()),
                Some(repo_root.to_path_buf()),
                LeaseRefreshPolicy::RefreshOnActivation,
                Some("web".to_owned()),
                Some(repo_root.to_path_buf()),
                RuntimeLeasePolicy::RefreshOnActivation,
            ))
        );
        assert!(activation.started_held);
    }

    #[test]
    fn inline_workspace_activation_keeps_the_stack_it_starts() {
        let repo_root = Path::new("/tmp/demo-repo");
        let policy = EffectiveContainerPolicy {
            render: None,
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "dev__app".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
            managed_volumes: vec![],
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services: vec![],
            project_name: "demo-web".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            runtime_host_key: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            workspace_source: effigy_containers::artifact_source::WorkspaceSource::Checkout,
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        };
        let mut activation_call = None;

        let activation = activate_inline_workspace_container_runtime_with(
            repo_root,
            &policy,
            |repo_root, policy, request, plan| {
                activation_call = Some((
                    repo_root.to_path_buf(),
                    policy.name.clone(),
                    request.container_name.map(str::to_owned),
                    request.repo_override,
                    request.session_context.lease_refresh_policy,
                    plan.request.container_name.clone(),
                    plan.request.repo_override.clone(),
                    plan.lease.policy,
                ));
                Ok(ContainerTaskActivation {
                    policy: policy.clone(),
                    system_was_running: false,
                    started_held: false,
                    hold: None,
                })
            },
        )
        .expect("activate inline workspace runtime");

        assert_eq!(
            activation_call,
            Some((
                repo_root.to_path_buf(),
                "dev__app".to_owned(),
                Some("dev__app".to_owned()),
                Some(repo_root.to_path_buf()),
                LeaseRefreshPolicy::SkipRefresh,
                Some("dev__app".to_owned()),
                Some(repo_root.to_path_buf()),
                RuntimeLeasePolicy::Skip,
            ))
        );
        assert!(!activation.started_held);
    }
}
