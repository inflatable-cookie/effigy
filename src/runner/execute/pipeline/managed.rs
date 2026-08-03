use super::super::super::container_command::{
    run_container_exec_operation_capture, support::validate_running_container_runtime_match,
};
use super::super::super::gateway_command::gateway_up_for_managed_task;
use super::super::super::locking::io::acquire_scopes;
use super::super::super::locking::model::LockScope;
use super::super::super::managed_shell::{
    managed_readiness_probe_urls, render_handoff_managed_lifecycle_command,
    render_inline_compose_command, render_inline_managed_lifecycle_command,
    render_inline_managed_shell_command, render_inline_managed_standard_exec_command,
};
use super::super::super::runtime_session_context::{
    current_runtime_session_context, LeaseRefreshPolicy, RuntimeSessionContext,
};
use super::super::super::system_command::run_workspace_with_repo_root_and_cleanup_override;
use super::super::api::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution,
    ContainerExecutionBinding, InlineWorkspaceCapabilitySurface,
};
use super::super::planning::ExecutionPreflight;
use crate::runner::container_runtime_prep::build_runtime_activation_plan;
use crate::runner::error::RunnerError;
use crate::runner::execute::sequence_run::run_in_process_sequence_steps;
use crate::runner::execute::workspace_seeded::{
    inside_container_handoff, run_workspace_seeded_task_session,
};
use effigy_containers::compose::compose_args;
use effigy_containers::session::{
    container_exec_command, managed_gateway_command, managed_lifecycle_command,
    managed_lifecycle_shutdown_command, managed_shell_command, managed_standard_exec_command,
    resolve_effigy_invocation_prefix,
};
use effigy_containers::{
    load_container_policy, ContainerCapturedExecOperation, EffectiveContainerPolicy,
};
use effigy_env::secret::SecretString;
use effigy_execution::{ExecutionBindingInput, ExecutionSelectionPlan};
use effigy_managed::command::resolve_managed_task_plan;
use effigy_managed::presentation::run_or_render_managed_task;
use effigy_managed::ManagedProcessRole;
use effigy_managed::{
    managed_execution_mode, render_run_step_sequence, wrap_command_with_env, ManagedExecutionMode,
};
use effigy_manifest::ManifestTaskSecretsMode;
use effigy_manifest::TaskSelection;
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeActivationRoute};
use std::collections::BTreeMap;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const MANAGED_LIFECYCLE_CLEANUP_TIMEOUT_SECS: u64 = 90;
pub(in crate::runner) fn run_managed_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &ExecutionSelectionPlan,
) -> Result<Option<String>, RunnerError> {
    let container_handoff = inside_container_handoff();
    let execution_mode = managed_execution_mode();
    let binding_resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "managed task execution",
    )?;
    let _binding_plan = binding_resolution.plan(ExecutionBindingInput::new(
        selection_plan.clone(),
        "managed task execution",
    ));
    let container_binding = binding_resolution.binding();
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
        if should_open_workspace_shell_for_non_managed_task(
            selection.task.run.is_some(),
            container_binding,
            container_handoff,
        ) {
            let ContainerExecutionBinding::Container { name, .. } = &container_binding else {
                unreachable!("workspace shell guard should only match named containers");
            };
            let lock_scopes = vec![crate::runner::manifest::task_lock_scope(
                selection.task,
                &preflight.selector,
            )];
            let _lock_guards = acquire_scopes(&preflight.resolved.resolved_root, &lock_scopes)?;
            let _ = name;
            return run_workspace_with_repo_root_and_cleanup_override(
                &preflight.resolved.resolved_root,
                selection.task.system.as_deref(),
                selection.task.workspace.as_deref(),
                preflight.runtime_args_raw.repo_override.clone(),
                preflight.output_json,
                Some(current_runtime_session_context().public_workspace_cleanup),
            )
            .map(Some);
        }
        ensure_inline_workspace_supported(
            container_binding,
            InlineWorkspaceCapabilitySurface::ManagedAttachedSession {
                task_name: &preflight.selector.task_name,
            },
        )?;
        return Ok(None);
    };
    let env_schema_resolved = super::standard::resolve_env_schema_if_present(
        &selection.catalog.catalog_root,
        preflight.runtime_args_raw.env_schema_override.as_deref(),
        selection.catalog.manifest.env_schema.as_ref(),
    )?;
    let container_repo_root =
        binding_resolution.exec_working_dir(&selection.catalog.catalog_root)?;
    let managed_task_secret_pairs = if execution_mode != ManagedExecutionMode::RenderPlan
        && matches!(
            selection.task.secrets,
            Some(ManifestTaskSecretsMode::Required)
        ) {
        super::standard::resolve_task_secret_env(
            &preflight.resolved.resolved_root,
            &preflight.secret_targets,
            selection.task,
            true,
        )?
    } else {
        Vec::new()
    };
    if execution_mode != ManagedExecutionMode::RenderPlan {
        run_managed_setup_steps(
            preflight,
            selection,
            &mut plan,
            &env_schema_resolved,
            &binding_resolution,
            container_binding,
            container_handoff,
            container_repo_root.as_deref(),
            &managed_task_secret_pairs,
        )?;
    }
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
        let _activation_plan = managed_runtime_activation_plan(
            &selection.catalog.catalog_root,
            &policy,
            container_binding.container_name(),
            current_runtime_session_context().lease_refresh_policy,
        );
        validate_running_container_runtime_match(&selection.catalog.catalog_root, &policy)?;
        maybe_start_managed_gateway(
            &selection.catalog.catalog_root,
            &preflight.selector.task_name,
            selection,
            plan.gateway_auto_start,
        )?;
        return run_workspace_seeded_task_session(
            &selection.catalog.catalog_root,
            container_binding,
            preflight.runtime_args_raw.repo_override.clone(),
            &preflight.selector.task_name,
            &preflight.runtime_args_exec.passthrough,
            Some(current_runtime_session_context().public_workspace_cleanup),
        )
        .map(Some);
    }
    materialize_special_managed_processes(
        &mut plan,
        preflight,
        selection,
        container_handoff,
        &managed_task_secret_pairs,
    )?;
    let role_schema_env = managed_role_schema_env(
        &preflight.catalogs,
        &selection.catalog.catalog_root,
        preflight.runtime_args_raw.env_schema_override.as_deref(),
    )?;
    apply_schema_env_to_managed_role_processes(
        &mut plan,
        &role_schema_env,
        &selection.catalog.catalog_root,
    );

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
            container_binding,
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

fn run_managed_setup_steps(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    plan: &mut effigy_managed::ManagedTaskPlan,
    env_schema_resolved: &Option<effigy_env::resolver::ResolvedEnv>,
    binding_resolution: &super::super::api::ExecutionBindingResolution,
    container_binding: &ContainerExecutionBinding,
    container_handoff: bool,
    container_repo_root: Option<&std::path::Path>,
    managed_task_secret_pairs: &[(String, SecretString)],
) -> Result<(), RunnerError> {
    let plan_has_lifecycle = plan
        .processes
        .iter()
        .any(|process| process.role == ManagedProcessRole::Lifecycle);
    let local_secret_refs = managed_task_secret_pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect::<Vec<_>>();
    let local_secret_ref = (!local_secret_refs.is_empty()).then_some(local_secret_refs.as_slice());
    for process in &mut plan.processes {
        if process.setup_steps.is_empty() {
            continue;
        }
        if managed_setup_steps_can_run_locally(process, container_binding, container_handoff) {
            run_in_process_sequence_steps(
                preflight,
                selection,
                &process.setup_steps,
                env_schema_resolved,
                local_secret_ref,
                &process.name,
                &[],
            )?;
            process.setup = None;
            continue;
        }
        if matches!(
            binding_resolution.kind(),
            super::super::api::ExecutionBindingKind::NamedContainer
        ) && !plan_has_lifecycle
        {
            run_named_container_managed_setup_steps(
                preflight,
                selection,
                process,
                container_binding,
                container_repo_root,
                managed_task_secret_pairs,
            )?;
            process.setup = None;
        }
    }
    Ok(())
}

fn run_named_container_managed_setup_steps(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    process: &effigy_managed::ManagedProcessSpec,
    container_binding: &ContainerExecutionBinding,
    container_repo_root: Option<&std::path::Path>,
    managed_task_secret_pairs: &[(String, SecretString)],
) -> Result<(), RunnerError> {
    let resolver: effigy_manifest::TaskResolverFn<'_> = &effigy_routing::resolve_task_selection;
    let rendered = render_run_step_sequence(
        &process.name,
        &process.setup_steps,
        &selection.task.env,
        selection.task.env_file.as_ref(),
        &selection.catalog.manifest.env,
        &selection.catalog.catalog_root,
        selection.catalog.bundle_root.as_deref(),
        &preflight.catalogs,
        &process.cwd,
        preflight.runtime_args_raw.env_schema_override.as_deref(),
        resolver,
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let rendered =
        crate::runner::secret_session::inject_secret_passphrase_into_internal_command(rendered);
    let rendered = wrap_with_managed_secret_env(
        rendered,
        managed_task_secret_pairs,
        &selection.catalog.catalog_root,
    );
    let routed = container_exec_command(
        &rendered,
        &selection.catalog.catalog_root,
        &process.cwd,
        container_repo_root,
    );
    let output = run_container_exec_operation_capture(
        &selection.catalog.catalog_root,
        container_binding.container_name(),
        ContainerCapturedExecOperation {
            service: None,
            command: vec!["sh".to_owned(), "-lc".to_owned(), routed.clone()],
            stdin_file: None,
            cwd: None,
            env: std::collections::BTreeMap::new(),
        },
    )?;
    if output.status.success() {
        return Ok(());
    }
    Err(RunnerError::TaskCommandFailure {
        command: routed,
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn managed_setup_steps_can_run_locally(
    process: &effigy_managed::ManagedProcessSpec,
    container_binding: &ContainerExecutionBinding,
    container_handoff: bool,
) -> bool {
    // Container-routed managed setup cannot move to the host fast path without
    // changing Rhai script semantics for `fs::*`, `process::*`, and relative
    // cwd handling. Those scripts still need a true local route.
    if process.run_on_host || container_handoff {
        return true;
    }

    matches!(
        container_binding,
        ContainerExecutionBinding::None | ContainerExecutionBinding::Host
    )
}

fn should_open_workspace_shell_for_non_managed_task(
    has_run_command: bool,
    container_binding: &ContainerExecutionBinding,
    container_handoff: bool,
) -> bool {
    if container_handoff || has_run_command {
        return false;
    }

    matches!(
        container_binding,
        ContainerExecutionBinding::Container { .. }
    )
}

fn build_managed_gateway_command(
    repo_root: &std::path::Path,
    task_name: &str,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let binding_resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        task_name,
        selection.task,
        "managed gateway startup",
    )?;
    let requested_container_name = match binding_resolution.kind() {
        super::super::api::ExecutionBindingKind::NamedContainer => {
            binding_resolution.requested_container_name()
        }
        super::super::api::ExecutionBindingKind::InlineContainer => {
            return Err(RunnerError::task_invocation(
                "`gateway = true` requires a workspace-backed container binding on the task",
            ));
        }
        super::super::api::ExecutionBindingKind::Host
        | super::super::api::ExecutionBindingKind::None => {
            return Err(RunnerError::task_invocation(
                "`gateway = true` requires a workspace-backed container binding on the task",
            ));
        }
    };
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

fn managed_runtime_activation_plan(
    repo_root: &std::path::Path,
    policy: &EffectiveContainerPolicy,
    container_name: Option<&str>,
    lease_refresh_policy: LeaseRefreshPolicy,
) -> RuntimeActivationPlan {
    build_runtime_activation_plan(
        repo_root,
        &policy.name,
        container_name,
        Some(repo_root.to_path_buf()),
        RuntimeActivationRoute::Managed,
        RuntimeSessionContext {
            lease_refresh_policy,
            ..RuntimeSessionContext::default()
        },
    )
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
    managed_task_secret_pairs: &[(String, SecretString)],
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
    let binding_resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "managed process materialization",
    )?;
    let container_binding = binding_resolution.binding();
    let inline_policy = if binding_resolution.is_inline_container() {
        binding_resolution.effective_policy(repo_root)?
    } else {
        None
    };
    let named_policy = if binding_resolution.is_inline_container() {
        None
    } else {
        binding_resolution.effective_policy(repo_root)?
    };
    if !container_handoff {
        if let Some(policy) = inline_policy.as_ref() {
            let _activation_plan = managed_runtime_activation_plan(
                repo_root,
                policy,
                Some(policy.name.as_str()),
                LeaseRefreshPolicy::SkipRefresh,
            );
            validate_running_container_runtime_match(repo_root, policy)?;
        } else if let Some(policy) = named_policy.as_ref() {
            let _activation_plan = managed_runtime_activation_plan(
                repo_root,
                policy,
                container_binding.container_name(),
                current_runtime_session_context().lease_refresh_policy,
            );
            validate_running_container_runtime_match(repo_root, policy)?;
        }
    }
    let ready_message = managed_ready_message(
        selection.task.ready_message.as_deref(),
        inline_policy.as_ref().or(named_policy.as_ref()),
    );
    let dns_route_lines = managed_dns_route_lines(inline_policy.as_ref().or(named_policy.as_ref()));
    let readiness_probe_urls =
        managed_readiness_probe_urls(inline_policy.as_ref().or(named_policy.as_ref()));
    let container_repo_root = binding_resolution.exec_working_dir(repo_root)?;
    for process in &mut plan.processes {
        match process.role {
            ManagedProcessRole::Lifecycle => {
                process.run = if container_handoff {
                    render_handoff_managed_lifecycle_command(
                        repo_root,
                        container_binding.container_name().unwrap_or("default"),
                        &preflight.selector.task_name,
                        selection.task.health_wait.unwrap_or(false),
                        ready_message.as_deref(),
                        &dns_route_lines,
                        &[],
                    )
                } else if let Some(policy) = inline_policy.as_ref() {
                    render_inline_managed_lifecycle_command(
                        repo_root,
                        policy,
                        &preflight.selector.task_name,
                        selection.task.health_wait.unwrap_or(false),
                        ready_message.as_deref(),
                        &dns_route_lines,
                        &readiness_probe_urls,
                        &[],
                    )
                } else {
                    managed_lifecycle_command(
                        repo_root,
                        container_binding.container_name(),
                        &preflight.selector.task_name,
                        selection.task.health_wait.unwrap_or(false),
                        ready_message.as_deref(),
                        &dns_route_lines,
                        &readiness_probe_urls,
                        &[],
                        &executable,
                        matches!(
                            selection.task.secrets,
                            Some(ManifestTaskSecretsMode::Required)
                        ),
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
                let wrapped_run = wrap_with_managed_secret_env(
                    crate::runner::secret_session::inject_secret_passphrase_into_internal_command(
                        process.run.clone(),
                    ),
                    managed_task_secret_pairs,
                    repo_root,
                );
                let wrapped_setup = process.setup.as_ref().map(|setup| {
                    wrap_with_managed_secret_env(
                        crate::runner::secret_session::inject_secret_passphrase_into_internal_command(
                            setup.clone(),
                        ),
                        managed_task_secret_pairs,
                        repo_root,
                    )
                });
                if process.run_on_host {
                    // Entry opts out of the parent task's container wrap —
                    // run the raw command on the host. The setup script,
                    // if any, runs in the same shell before the run.
                    if let Some(setup) = wrapped_setup.as_deref() {
                        process.run = format!("{setup}\n{wrapped_run}");
                    } else {
                        process.run = wrapped_run;
                    }
                    continue;
                }
                if container_handoff {
                    process.run = render_handoff_managed_standard_command(
                        wrapped_setup.as_deref(),
                        &wrapped_run,
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
                        wrapped_setup.as_deref(),
                        &wrapped_run,
                    );
                } else if container_binding.container_name().is_some() {
                    process.run = managed_standard_exec_command(
                        repo_root,
                        container_binding.container_name(),
                        &preflight.selector.task_name,
                        &process.cwd,
                        container_repo_root.as_deref(),
                        wrapped_setup.as_deref(),
                        &executable,
                        &wrapped_run,
                    );
                }
            }
        }
    }
    Ok(())
}

fn wrap_with_managed_secret_env(
    command: String,
    managed_task_secret_pairs: &[(String, SecretString)],
    repo_root: &std::path::Path,
) -> String {
    if managed_task_secret_pairs.is_empty() {
        return command;
    }
    let env = managed_task_secret_pairs
        .iter()
        .map(|(key, value)| (key.clone(), value.expose().to_owned()))
        .collect::<BTreeMap<String, String>>();
    wrap_command_with_env(command, &env, repo_root)
}

/// Resolves the catalog env schema plain env for shell/lifecycle role
/// processes, inheriting the nearest ancestor catalog's `[env_schema]` when
/// the selection catalog declares none (same rule as task rendering).
fn managed_role_schema_env(
    catalogs: &[effigy_manifest::LoadedCatalog],
    catalog_root: &std::path::Path,
    runtime_env_schema_override: Option<&std::path::Path>,
) -> Result<BTreeMap<String, String>, RunnerError> {
    let declaring = effigy_manifest::env_schema_declaring_catalog(catalogs, catalog_root);
    let resolved = super::standard::resolve_env_schema_if_present(
        declaring.map_or(catalog_root, |catalog| catalog.catalog_root.as_path()),
        runtime_env_schema_override,
        declaring.and_then(|catalog| catalog.manifest.env_schema.as_ref()),
    )?;
    Ok(resolved
        .map(|resolved| resolved.plain_env())
        .unwrap_or_default())
}

/// Shell and lifecycle role processes are not task references, so they never
/// pass through task-render env. Fold the resolved schema env into their run
/// commands with the same `env 'KEY=VALUE' ...` prefix used for vault
/// secrets; standard (task) processes already carry schema env from render.
fn apply_schema_env_to_managed_role_processes(
    plan: &mut effigy_managed::ManagedTaskPlan,
    schema_env: &BTreeMap<String, String>,
    repo_root: &std::path::Path,
) {
    if schema_env.is_empty() {
        return;
    }
    for process in &mut plan.processes {
        if matches!(
            process.role,
            ManagedProcessRole::Shell | ManagedProcessRole::Lifecycle
        ) {
            process.run = wrap_command_with_env(process.run.clone(), schema_env, repo_root);
        }
    }
}

fn managed_ready_message(
    explicit_ready_message: Option<&str>,
    policy: Option<&EffectiveContainerPolicy>,
) -> Option<String> {
    if let Some(ready_message) = explicit_ready_message
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(ready_message.to_owned());
    }

    let policy = policy?;
    let mut seen = std::collections::HashSet::new();
    let urls = policy
        .dns_routes
        .iter()
        .filter_map(|route| {
            let scheme = if route.tls { "https" } else { "http" };
            let url = format!("{scheme}://{}", route.domain);
            seen.insert(url.clone()).then_some(url)
        })
        .collect::<Vec<_>>();

    if urls.is_empty() {
        None
    } else {
        Some(format!("routes: {}", urls.join(" | ")))
    }
}

fn managed_dns_route_lines(policy: Option<&EffectiveContainerPolicy>) -> Vec<String> {
    let Some(policy) = policy else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut routes = Vec::new();
    for route in &policy.dns_routes {
        let domain = route.domain.trim();
        if domain.is_empty() {
            continue;
        }
        let scheme = if route.tls { "https" } else { "http" };
        let service = route.service.as_deref().unwrap_or(&policy.primary_service);
        let mut line = format!("{scheme}://{domain} -> {service}");
        if let Some(port) = route.port {
            line.push_str(&format!(":{port}"));
        }
        if seen.insert(line.clone()) {
            routes.push(line);
        }
    }

    let Some(base_domain) = policy
        .dns_routes
        .first()
        .and_then(|route| base_domain_from_dns_route(&route.domain))
    else {
        return routes;
    };

    let explicit_domains = policy
        .dns_routes
        .iter()
        .map(|route| route.domain.as_str())
        .collect::<std::collections::HashSet<_>>();
    for alias in &policy.service_aliases {
        let domain = format!("{}.{}", alias.domain_label, base_domain);
        if explicit_domains.contains(domain.as_str()) {
            continue;
        }
        let line = format!("{domain}:{} -> {}", alias.container_port, alias.service);
        if seen.insert(line.clone()) {
            routes.push(line);
        }
    }

    routes
}

fn base_domain_from_dns_route(domain: &str) -> Option<&str> {
    let domain = domain.trim();
    if domain.is_empty() {
        return None;
    }
    let mut labels = domain.split('.').filter(|part| !part.is_empty());
    labels.next()?;
    labels.next()?;
    Some(domain)
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

fn default_handoff_managed_shell_run() -> String {
    "if [ -n \"${SHELL:-}\" ] && [ -x \"${SHELL}\" ]; then exec \"${SHELL}\" -i; fi; if command -v bash >/dev/null 2>&1; then exec \"$(command -v bash)\" -i; fi; if command -v sh >/dev/null 2>&1; then exec \"$(command -v sh)\" -i; fi; exec /bin/sh -i".to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_schema_env_to_managed_role_processes, default_handoff_managed_shell_run,
        finish_managed_task, managed_dns_route_lines, managed_role_schema_env,
        managed_runtime_activation_plan, render_handoff_managed_standard_command,
        render_managed_lifecycle_cleanup_notice, should_open_workspace_shell_for_non_managed_task,
        ContainerExecutionBinding, ManagedProcessRole,
    };
    use crate::runner::error::RunnerError;
    use crate::runner::execute::workspace_seeded::render_workspace_seeded_task_command;
    use crate::runner::managed_shell::{
        managed_readiness_probe_urls, render_inline_managed_standard_exec_command,
    };
    use crate::runner::runtime_session_context::LeaseRefreshPolicy;
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, EffectiveDnsRoute, EffectiveServiceAlias,
    };
    use effigy_manifest::{
        LoadedCatalog, ManifestContainerDriver, ManifestContainerOnTaskExit,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };
    use effigy_runtime_plan::{RuntimeActivationRoute, RuntimeLeasePolicy};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;

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

        assert!(
            rendered.ends_with("effigy 'dev' 'front' '--' '--host' '0.0.0.0'"),
            "got: {rendered}"
        );
    }

    #[test]
    fn non_managed_container_task_with_run_skips_workspace_shell_handoff() {
        assert!(!should_open_workspace_shell_for_non_managed_task(
            true,
            &ContainerExecutionBinding::Container {
                name: Some("web".to_owned()),
                workspace: None,
            },
            false,
        ));
    }

    #[test]
    fn non_managed_container_task_without_run_opens_workspace_shell() {
        assert!(should_open_workspace_shell_for_non_managed_task(
            false,
            &ContainerExecutionBinding::Container {
                name: Some("web".to_owned()),
                workspace: None,
            },
            false,
        ));
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

    #[test]
    fn managed_runtime_activation_plan_keeps_identity_and_lease_policy() {
        let plan = managed_runtime_activation_plan(
            Path::new("/tmp/repo"),
            &test_policy(),
            Some("stack"),
            LeaseRefreshPolicy::RefreshOnActivation,
        );

        assert_eq!(plan.request.repo_root, Path::new("/tmp/repo"));
        assert_eq!(plan.request.policy_name, "stack");
        assert_eq!(plan.request.container_name.as_deref(), Some("stack"));
        assert_eq!(
            plan.request.repo_override.as_deref(),
            Some(Path::new("/tmp/repo"))
        );
        assert_eq!(plan.route, RuntimeActivationRoute::Managed);
        assert_eq!(plan.lease.policy, RuntimeLeasePolicy::RefreshOnActivation);
    }

    #[test]
    fn managed_inline_activation_plan_can_skip_host_lease_refresh() {
        let plan = managed_runtime_activation_plan(
            Path::new("/tmp/repo"),
            &test_policy(),
            Some("stack"),
            LeaseRefreshPolicy::SkipRefresh,
        );

        assert_eq!(plan.lease.policy, RuntimeLeasePolicy::Skip);
    }

    #[test]
    fn managed_activation_plan_preserves_policy_identity_without_container_name() {
        let plan = managed_runtime_activation_plan(
            Path::new("/tmp/repo"),
            &test_policy(),
            None,
            LeaseRefreshPolicy::SkipRefresh,
        );

        assert_eq!(plan.request.repo_root, Path::new("/tmp/repo"));
        assert_eq!(plan.request.policy_name, "stack");
        assert_eq!(plan.request.container_name, None);
        assert_eq!(plan.route, RuntimeActivationRoute::Managed);
        assert_eq!(
            plan.request.repo_override.as_deref(),
            Some(Path::new("/tmp/repo"))
        );
        assert_eq!(plan.lease.policy, RuntimeLeasePolicy::Skip);
    }

    #[test]
    fn managed_dns_route_lines_include_http_routes_and_service_aliases() {
        let mut policy = test_policy();
        policy.primary_service = "app".to_owned();
        policy.dns_routes = vec![
            EffectiveDnsRoute {
                domain: "project.test".to_owned(),
                tls: false,
                port: None,
                service: None,
                target_host: None,
            },
            EffectiveDnsRoute {
                domain: "admin.project.test".to_owned(),
                tls: true,
                port: Some(41002),
                service: Some("admin".to_owned()),
                target_host: None,
            },
        ];
        policy.service_aliases = vec![EffectiveServiceAlias {
            service: "postgres".to_owned(),
            domain_label: "postgres".to_owned(),
            container_port: 5432,
        }];

        let routes = managed_dns_route_lines(Some(&policy));

        assert_eq!(
            routes,
            vec![
                "http://project.test -> app".to_owned(),
                "https://admin.project.test -> admin:41002".to_owned(),
                "postgres.project.test:5432 -> postgres".to_owned(),
            ]
        );
    }

    #[test]
    fn managed_readiness_probe_urls_follow_dns_routes_only() {
        let mut policy = test_policy();
        policy.dns_routes = vec![
            EffectiveDnsRoute {
                domain: "project.test".to_owned(),
                tls: false,
                port: None,
                service: None,
                target_host: None,
            },
            EffectiveDnsRoute {
                domain: "admin.project.test".to_owned(),
                tls: true,
                port: Some(41002),
                service: Some("admin".to_owned()),
                target_host: None,
            },
        ];

        let urls = managed_readiness_probe_urls(Some(&policy));

        assert_eq!(
            urls,
            vec![
                "http://project.test".to_owned(),
                "https://admin.project.test".to_owned(),
            ]
        );
    }

    fn managed_role_schema_test_repo(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "effigy-managed-role-schema-{name}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("mkdir temp repo");
        root
    }

    fn load_schema_test_catalog(catalog_root: &Path, alias: &str, depth: usize) -> LoadedCatalog {
        let manifest_path = catalog_root.join("effigy.toml");
        LoadedCatalog {
            alias: alias.to_owned(),
            catalog_root: catalog_root.to_path_buf(),
            manifest: effigy_manifest::load_task_manifest(&manifest_path).expect("manifest"),
            manifest_path,
            bundle_root: None,
            defer_run: None,
            deferred_builtins: std::collections::BTreeSet::new(),
            depth,
        }
    }

    #[test]
    fn managed_role_schema_env_inherits_ancestor_catalog_env_schema() {
        let root = managed_role_schema_test_repo("ancestor");
        let child = root.join("cp-api");
        fs::create_dir_all(&child).expect("mkdir child catalog");
        fs::write(
            root.join("effigy.toml"),
            "[env_schema]\nschema = \"dev.env.schema\"\n",
        )
        .expect("root manifest");
        fs::write(root.join("dev.env.schema"), "ENVIRONMENT=effigy\n").expect("write schema");
        fs::write(
            child.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf ok\"\n",
        )
        .expect("child manifest");
        let catalogs = vec![
            load_schema_test_catalog(&root, "root", 0),
            load_schema_test_catalog(&child, "cp-api", 1),
        ];

        let env = managed_role_schema_env(&catalogs, &child, None).expect("resolve schema env");

        assert_eq!(env.get("ENVIRONMENT").map(String::as_str), Some("effigy"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn managed_role_schema_env_is_empty_without_any_schema() {
        let root = managed_role_schema_test_repo("none");
        fs::write(
            root.join("effigy.toml"),
            "[tasks.dev]\nrun = \"printf ok\"\n",
        )
        .expect("manifest");
        let catalogs = vec![load_schema_test_catalog(&root, "root", 0)];

        let env = managed_role_schema_env(&catalogs, &root, None).expect("resolve schema env");

        assert!(env.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    fn role_process(
        name: &str,
        role: ManagedProcessRole,
        run: &str,
    ) -> effigy_managed::ManagedProcessSpec {
        effigy_managed::ManagedProcessSpec {
            name: name.to_owned(),
            role,
            run: run.to_owned(),
            setup: None,
            setup_steps: Vec::new(),
            cwd: PathBuf::from("/tmp/repo"),
            service: None,
            start_after_ms: 0,
            shutdown_on_exit: false,
            run_on_host: false,
        }
    }

    fn role_process_plan(
        processes: Vec<effigy_managed::ManagedProcessSpec>,
    ) -> effigy_managed::ManagedTaskPlan {
        effigy_managed::ManagedTaskPlan {
            mode: "tui".to_owned(),
            profile: "dev".to_owned(),
            processes,
            tab_order: Vec::new(),
            fail_on_non_zero: false,
            passthrough: Vec::new(),
            gateway_auto_start: false,
            readiness: effigy_managed::ManagedTaskReadiness::default(),
        }
    }

    #[test]
    fn shell_and_lifecycle_role_processes_gain_schema_env_prefix() {
        let mut plan = role_process_plan(vec![
            role_process(
                "shell",
                ManagedProcessRole::Shell,
                "exec ${SHELL:-/bin/zsh} -i",
            ),
            role_process(
                "lifecycle",
                ManagedProcessRole::Lifecycle,
                "effigy container up --detach",
            ),
            role_process("api", ManagedProcessRole::Standard, "cargo run -p cp-api"),
        ]);
        let env = BTreeMap::from([("ENVIRONMENT".to_owned(), "effigy".to_owned())]);

        apply_schema_env_to_managed_role_processes(&mut plan, &env, Path::new("/tmp/repo"));

        let shell = &plan.processes[0].run;
        assert!(
            shell.starts_with("env 'ENVIRONMENT=effigy' sh -c"),
            "got: {shell}"
        );
        assert!(shell.contains("exec ${SHELL:-/bin/zsh} -i"), "got: {shell}");
        let lifecycle = &plan.processes[1].run;
        assert!(
            lifecycle.starts_with("env 'ENVIRONMENT=effigy' sh -c"),
            "got: {lifecycle}"
        );
        assert_eq!(plan.processes[2].run, "cargo run -p cp-api");
    }

    #[test]
    fn role_processes_stay_unchanged_without_schema_env() {
        let mut plan = role_process_plan(vec![role_process(
            "shell",
            ManagedProcessRole::Shell,
            "exec ${SHELL:-/bin/zsh} -i",
        )]);

        apply_schema_env_to_managed_role_processes(
            &mut plan,
            &BTreeMap::new(),
            Path::new("/tmp/repo"),
        );

        assert_eq!(plan.processes[0].run, "exec ${SHELL:-/bin/zsh} -i");
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
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }
}
