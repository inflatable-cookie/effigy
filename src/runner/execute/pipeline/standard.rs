use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::super::super::cache::ops::check_task_cache;
use super::super::super::exec_command::{
    capture_routed_task_container_exec, capture_routed_task_container_exec_with_policy,
    run_routed_task_container_exec, run_routed_task_container_exec_with_policy,
    RoutedTaskExecRequest,
};
use super::super::super::locking::io::acquire_scopes;
use super::super::super::system_command::is_primary_service_running;
use super::super::api::{
    effective_task_binding_inputs, execution_scope_root, resolve_execution_binding_resolution,
    ContainerExecutionBinding, ExecutionBindingResolution,
};
use super::super::context::ExecutionTaskContext;
use super::super::planning::ExecutionPreflight;
use super::super::routing::{
    route_standard_task_execution, routed_container_target, routed_not_running_container,
    RoutedTaskExecution,
};
use super::super::task_status::{
    container_route_summary, host_route_summary, inline_route_summary, pending_route_summary,
    TaskStatusTracker,
};
use super::{super::process_run, command};
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, build_runtime_activation_plan, ActivationRequest,
};
use crate::runner::error::RunnerError;
use crate::runner::execute::nested;
use crate::runner::execute::render;
use crate::runner::execute::workspace_seeded::{
    inside_container_handoff, run_workspace_seeded_task_session,
};
use crate::runner::host_container_lease::emit_host_container_lease_notice;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};
use crate::runner::runtime_session_context::{
    current_runtime_session_context, LeaseRefreshPolicy, RuntimeSessionContext,
};
use effigy_cli::{SecretsArgs, SecretsSubcommand};
use effigy_containers::compose::compose_args;
use effigy_containers::exec::run_compose_capture;
use effigy_containers::load_container_policy;
use effigy_env::resolver::ResolvedEnv;
use effigy_env::schema_support::{
    resolve_catalog_env_schema as shared_resolve_env_schema, SchemaSupportConfig,
    SchemaSupportError,
};
use effigy_env::secret::SecretString;
use effigy_execution::{ExecutionBindingInput, ExecutionSelectionPlan, TaskStatusStage};
use effigy_manifest::{
    ManifestManagedRun, ManifestManagedRunStep, ManifestSecretTarget, ManifestSecretsBackend,
    ManifestSecretsConfig, ManifestTaskRunIn, TaskSelection,
};
use effigy_runtime_plan::{RuntimeActivationPlan, RuntimeActivationRoute};
use effigy_secrets::{SecretValue, VaultPlaintextPayload};

pub(in crate::runner) fn run_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &ExecutionSelectionPlan,
) -> Result<String, RunnerError> {
    let env_schema_catalog = effigy_manifest::env_schema_declaring_catalog(
        &preflight.catalogs,
        &selection.catalog.catalog_root,
    );
    let env_schema_resolved = resolve_env_schema_if_present(
        env_schema_catalog.map_or(selection.catalog.catalog_root.as_path(), |catalog| {
            catalog.catalog_root.as_path()
        }),
        preflight.runtime_args_raw.env_schema_override.as_deref(),
        env_schema_catalog.and_then(|catalog| catalog.manifest.env_schema.as_ref()),
    )?;

    let context = ExecutionTaskContext::new(
        preflight,
        selection,
        command::build_task_command(preflight, selection, &env_schema_resolved)?,
    );

    let lock_scope = crate::runner::manifest::task_lock_scope(selection.task, &preflight.selector);
    let mut status = TaskStatusTracker::start(preflight, selection_plan, vec![lock_scope.label()])?;
    status.update_stage(TaskStatusStage::WaitingForLock, pending_route_summary())?;

    let result = run_standard_task_inner(
        preflight,
        selection,
        selection_plan,
        &context,
        &env_schema_resolved,
        &lock_scope,
        &mut status,
    );
    match result {
        Ok((output, summary)) => {
            status.finish_success(summary)?;
            Ok(output)
        }
        Err(error) => {
            status.finish_error(&error)?;
            Err(error)
        }
    }
}

fn run_standard_task_inner(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &ExecutionSelectionPlan,
    context: &ExecutionTaskContext<'_>,
    env_schema_resolved: &Option<ResolvedEnv>,
    lock_scope: &crate::runner::locking::model::LockScope,
    status: &mut TaskStatusTracker,
) -> Result<(String, String), RunnerError> {
    let _lock_guards = acquire_scopes(
        &preflight.resolved.resolved_root,
        std::slice::from_ref(lock_scope),
    )?;

    let cache_check = check_task_cache(
        &preflight.resolved.resolved_root,
        &selection.catalog.catalog_root,
        &selection.catalog.manifest_path,
        &preflight.selector.task_name,
        selection.task,
        context.command(),
    )?;
    if cache_check.enabled && cache_check.hit {
        let output = render::render_cache_hit_output(
            preflight.output_json,
            preflight.runtime_args_raw.verbose_root,
            context,
            &cache_check.reason,
            &cache_check.fingerprint,
        )?;
        return Ok((output, format!("cache hit ({})", cache_check.reason)));
    }

    let env_schema_secret_pairs = env_schema_resolved
        .as_ref()
        .map(|resolved| resolved.secret_env())
        .unwrap_or_default();
    let task_secret_pairs = if task_uses_direct_shell(selection.task.run.as_ref()) {
        resolve_task_secret_env(
            &preflight.resolved.resolved_root,
            &preflight.secret_targets,
            selection.task,
            matches!(
                selection.task.secrets,
                Some(effigy_manifest::ManifestTaskSecretsMode::Required)
            ),
            preflight.selector.task_name == "dev",
        )?
    } else {
        Vec::new()
    };
    let mut secret_pairs = env_schema_secret_pairs;
    for (key, value) in &task_secret_pairs {
        secret_pairs.push((key.as_str(), value));
    }
    let secret_ref = (!secret_pairs.is_empty()).then_some(secret_pairs.as_slice());

    if let Some(output) = nested::maybe_run_fully_in_process_sequence(
        preflight,
        selection,
        context,
        env_schema_resolved,
        secret_ref,
    )? {
        status.update_stage(TaskStatusStage::Executing, host_route_summary())?;
        return Ok((output, "in-process task sequence completed".to_owned()));
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
    if let ContainerExecutionBinding::Inline { synthetic_name, .. } = container_binding {
        status.update_stage(
            TaskStatusStage::Handoff,
            inline_route_summary(synthetic_name),
        )?;
        let output = run_inline_workspace_standard_task(
            preflight,
            selection,
            context,
            secret_ref,
            &binding_resolution,
        )?;
        return Ok((output, "inline workspace task completed".to_owned()));
    }

    let routed = route_with_running_check(
        scope_root,
        preflight,
        selection,
        default_run_in,
        systems.as_ref(),
        containers.as_ref(),
    )?;
    let initial_route = routed_container_target(&routed.decision)
        .map(|(container, service)| container_route_summary(container, service))
        .unwrap_or_else(host_route_summary);
    status.update_stage(TaskStatusStage::RuntimePrep, initial_route)?;

    let mut task_activation = None;
    let routed = if let Some(container_name) = routed_not_running_container(&routed.decision) {
        task_activation = Some(activate_routed_container_runtime(
            scope_root,
            container_name,
        )?);
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
            task_activation = Some(activate_routed_container_runtime(
                scope_root,
                container_name,
            )?);
        }
        routed
    };

    if should_stay_in_workspace_shell(preflight.output_json, selection.task, container_binding) {
        status.update_stage(
            TaskStatusStage::ManagedSession,
            routed_container_target(&routed.decision)
                .map(|(container, service)| container_route_summary(container, service))
                .unwrap_or_else(host_route_summary),
        )?;
        let output = run_workspace_seeded_task_session(
            &selection.catalog.catalog_root,
            container_binding,
            preflight.runtime_args_raw.repo_override.clone(),
            &preflight.selector.task_name,
            &preflight.runtime_args_exec.passthrough,
            Some(current_runtime_session_context().public_workspace_cleanup),
        )?;
        return Ok((output, "workspace-seeded task session completed".to_owned()));
    }

    if let Some((container, service)) = routed_container_target(&routed.decision) {
        status.update_stage(
            TaskStatusStage::Executing,
            container_route_summary(container, service),
        )?;
        if preflight.output_json {
            let output = capture_routed_task_container_exec(
                RoutedTaskExecRequest {
                    repo_root: scope_root,
                    invocation_cwd: &preflight.invocation_cwd,
                    selector: &preflight.selector,
                    task_args: &preflight.runtime_args_exec.passthrough,
                    service,
                    command: context.command(),
                    task_env: Some(&context.selection.task.env),
                    secret_env: secret_ref,
                },
                container,
            )?;
            let stdout =
                redact_task_secret_values(&String::from_utf8_lossy(&output.stdout), secret_ref);
            let stderr =
                redact_task_secret_values(&String::from_utf8_lossy(&output.stderr), secret_ref);
            let rendered = render::render_task_command_json(
                &preflight.selector.task_name,
                &preflight.selector,
                context.repo_for_task(),
                context.command(),
                output.status.code(),
                &stdout,
                &stderr,
            )?;
            if output.status.success() {
                if task_activation
                    .is_some_and(|activation| activation.refreshed_host_container_lease)
                {
                    emit_host_container_lease_notice(container);
                }
                return Ok((rendered, "container task completed".to_owned()));
            }
            return Err(RunnerError::CommandJsonFailure { rendered });
        }

        run_routed_task_container_exec(
            RoutedTaskExecRequest {
                repo_root: scope_root,
                invocation_cwd: &preflight.invocation_cwd,
                selector: &preflight.selector,
                task_args: &preflight.runtime_args_exec.passthrough,
                service,
                command: context.command(),
                task_env: Some(&context.selection.task.env),
                secret_env: secret_ref,
            },
            container,
        )?;
        if task_activation.is_some_and(|activation| activation.refreshed_host_container_lease) {
            emit_host_container_lease_notice(container);
        }
        if preflight.runtime_args_raw.verbose_root {
            return Ok((
                context.render_resolution_trace(),
                "container task completed".to_owned(),
            ));
        }
        return Ok((String::new(), "container task completed".to_owned()));
    }

    status.update_stage(TaskStatusStage::Executing, host_route_summary())?;
    if let Some(output) = nested::maybe_run_in_process_sequence(
        preflight,
        selection,
        context,
        env_schema_resolved,
        secret_ref,
    )? {
        return Ok((output, "nested task sequence completed".to_owned()));
    }

    let output = process_run::run_task_process(
        preflight.output_json,
        preflight.runtime_args_raw.verbose_root,
        context,
        secret_ref,
    )?;
    Ok((output, "host task completed".to_owned()))
}

fn task_uses_direct_shell(run: Option<&ManifestManagedRun>) -> bool {
    match run {
        Some(ManifestManagedRun::Command(command)) => command
            .strip_prefix("task:")
            .map(str::trim)
            .is_none_or(|value| value.is_empty()),
        Some(ManifestManagedRun::Sequence(steps)) => steps.iter().any(step_uses_direct_shell),
        None => false,
    }
}

fn step_uses_direct_shell(step: &ManifestManagedRunStep) -> bool {
    match step {
        ManifestManagedRunStep::Command(command) => command
            .strip_prefix("task:")
            .map(str::trim)
            .is_none_or(|value| value.is_empty()),
        ManifestManagedRunStep::Step(table) => table.run.is_some(),
    }
}

fn referenced_task_secret_env_names(task: &effigy_manifest::ManifestTask) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(run) = task.run.as_ref() {
        collect_task_secret_env_names_from_run(run, &mut names);
    }
    names
}

fn collect_task_secret_env_names_from_run(run: &ManifestManagedRun, names: &mut BTreeSet<String>) {
    match run {
        ManifestManagedRun::Command(command) => {
            collect_task_secret_env_names_from_shell(command, names);
        }
        ManifestManagedRun::Sequence(steps) => {
            for step in steps {
                match step {
                    ManifestManagedRunStep::Command(command) => {
                        collect_task_secret_env_names_from_shell(command, names);
                    }
                    ManifestManagedRunStep::Step(table) => {
                        if let Some(run) = table.run.as_deref() {
                            collect_task_secret_env_names_from_shell(run, names);
                        }
                        if let Some(effigy_manifest::ManifestRunStepEnv::Inline(env)) =
                            table.env.as_ref()
                        {
                            names.extend(env.keys().cloned());
                        }
                    }
                }
            }
        }
    }
}

fn collect_task_secret_env_names_from_shell(command: &str, names: &mut BTreeSet<String>) {
    let bytes = command.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        if index + 1 >= bytes.len() {
            break;
        }
        if bytes[index + 1] == b'{' {
            let start = index + 2;
            let mut end = start;
            while end < bytes.len() && is_env_name_char(bytes[end]) {
                end += 1;
            }
            if end > start && end < bytes.len() && bytes[end] == b'}' {
                names.insert(command[start..end].to_owned());
                index = end + 1;
                continue;
            }
        } else if is_env_name_start(bytes[index + 1]) {
            let start = index + 1;
            let mut end = start + 1;
            while end < bytes.len() && is_env_name_char(bytes[end]) {
                end += 1;
            }
            names.insert(command[start..end].to_owned());
            index = end;
            continue;
        }
        index += 1;
    }
}

fn is_env_name_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_env_name_char(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
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

pub(in crate::runner::execute) fn resolve_task_secret_env(
    repo_root: &Path,
    extra_targets: &[String],
    task: &effigy_manifest::ManifestTask,
    eager_load: bool,
    local_dev: bool,
) -> Result<Vec<(String, SecretString)>, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(Vec::new());
    };
    let targets = task_secret_targets(extra_targets)?;
    let requested_env_names = referenced_task_secret_env_names(task);
    if requested_env_names.is_empty() && !eager_load {
        return Ok(Vec::new());
    }
    let task_keys = secrets
        .keys
        .iter()
        .filter(|(name, key)| {
            key.targets.iter().any(|target| targets.contains(target))
                && (eager_load || requested_env_names.contains(&task_secret_env_name(name)))
        })
        .collect::<Vec<_>>();
    if task_keys.is_empty() {
        return Ok(Vec::new());
    }

    let required_names = task_keys
        .iter()
        .filter(|(_, key)| key.required)
        .map(|(name, _)| (*name).clone())
        .collect::<Vec<_>>();
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        if required_names.is_empty() {
            return Ok(Vec::new());
        }
        return Err(RunnerError::task_invocation(
            "required task secrets need `[secrets].backend = \"effigy-vault\"`",
        ));
    }

    let vault_path = resolve_task_secret_vault_path(repo_root, secrets)?;
    if !vault_path.exists() && eager_load {
        crate::runner::run_command(effigy_cli::Command::Secrets(SecretsArgs {
            subcommand: SecretsSubcommand::Init,
            repo_override: Some(repo_root.to_path_buf()),
            output_json: false,
        }))?;
        if required_names.is_empty() {
            return Ok(Vec::new());
        }
        if !vault_path.exists() {
            return Err(RunnerError::task_invocation(format!(
                "required task secrets are declared but the vault is missing at {}",
                vault_path.display()
            )));
        }
    }

    let mut payload = if local_dev {
        match crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path) {
            Ok(payload) => payload,
            Err(_) => {
                let Some(passphrase) =
                    read_local_dev_upgrade_passphrase(required_names.is_empty())?
                else {
                    return Ok(Vec::new());
                };
                let payload = read_task_secret_vault_payload(&vault_path, passphrase.expose())?;
                crate::runner::secret_vault::write_effigy_vault_payload(
                    &vault_path,
                    &payload,
                    passphrase.expose(),
                )?;
                payload
            }
        }
    } else {
        let Some(passphrase) = read_task_secret_passphrase(required_names.is_empty())? else {
            return Ok(Vec::new());
        };
        read_task_secret_vault_payload(&vault_path, passphrase.expose())?
    };

    let mut missing_required =
        task_required_secret_names_missing_from_payload(&payload, &task_keys);
    if !missing_required.is_empty() && eager_load {
        maybe_generate_required_task_secrets(repo_root, secrets)?;
        payload = if local_dev {
            crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path)?
        } else {
            let Some(passphrase) = read_task_secret_passphrase(false)? else {
                unreachable!("required task secret generation needs an unlock passphrase")
            };
            read_task_secret_vault_payload(&vault_path, passphrase.expose())?
        };
        missing_required = task_required_secret_names_missing_from_payload(&payload, &task_keys);
    }

    let mut injected = Vec::new();
    for (name, key) in task_keys {
        match payload.records.get(name.as_str()) {
            Some(record) => injected.push((
                task_secret_env_name(name),
                SecretString::new(record.value.expose().to_owned()),
            )),
            None if key.required => missing_required.push(name.to_owned()),
            None => {}
        }
    }

    if !missing_required.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "required task secret(s) missing from the vault: {}",
            missing_required.join(", ")
        )));
    }

    Ok(injected)
}

fn task_required_secret_names_missing_from_payload(
    payload: &VaultPlaintextPayload,
    task_keys: &[(&String, &effigy_manifest::ManifestSecretKeyConfig)],
) -> Vec<String> {
    task_keys
        .iter()
        .filter(|(name, key)| key.required && !payload.records.contains_key(name.as_str()))
        .map(|(name, _)| (*name).clone())
        .collect()
}

fn task_secret_targets(extra_targets: &[String]) -> Result<Vec<ManifestSecretTarget>, RunnerError> {
    let mut targets = vec![ManifestSecretTarget::Tasks];
    for target in extra_targets {
        let target = match target.as_str() {
            "tasks" => ManifestSecretTarget::Tasks,
            "containers" => ManifestSecretTarget::Containers,
            "rhai" => ManifestSecretTarget::Rhai,
            "deploy" => ManifestSecretTarget::Deploy,
            "state" => ManifestSecretTarget::State,
            "artifacts" => ManifestSecretTarget::Artifacts,
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "unknown execution secret target `{other}`"
                )));
            }
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

fn resolve_task_secret_vault_path(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
) -> Result<PathBuf, RunnerError> {
    crate::runner::secret_vault::resolve_effigy_vault_read_path(
        repo_root,
        secrets,
        "task secret injection",
    )
}

fn maybe_generate_required_task_secrets(
    repo_root: &Path,
    secrets: &ManifestSecretsConfig,
) -> Result<(), RunnerError> {
    crate::runner::secrets_command::run_configured_vault_generate_task(repo_root, Some(secrets))
        .map(|_| ())
}

fn read_task_secret_passphrase(optional_only: bool) -> Result<Option<SecretValue>, RunnerError> {
    crate::runner::secret_session::read_secret_passphrase(
        optional_only,
        "Vault passphrase: ",
        "task secrets require an unlocked vault passphrase and secret input requires an interactive TTY",
    )
}

fn read_local_dev_upgrade_passphrase(
    optional_only: bool,
) -> Result<Option<SecretValue>, RunnerError> {
    crate::runner::secret_session::read_local_dev_upgrade_passphrase(
        optional_only,
        "Vault passphrase (one-time local-dev setup): ",
        "local-dev secrets need one passphrase unlock to create the unattended dev key, and secret input requires an interactive TTY",
    )
}

fn read_task_secret_vault_payload(
    vault_path: &Path,
    passphrase: &str,
) -> Result<VaultPlaintextPayload, RunnerError> {
    crate::runner::secret_vault::read_effigy_vault_payload(vault_path, passphrase)
}

fn task_secret_env_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

pub(in crate::runner::execute) fn redact_task_secret_values(
    input: &str,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> String {
    let Some(secret_env) = secret_env else {
        return input.to_owned();
    };
    let mut redacted = input.to_owned();
    for (_, secret) in secret_env {
        let value = secret.expose();
        if !value.is_empty() {
            redacted = redacted.replace(value, "[REDACTED]");
        }
    }
    redacted
}

fn activate_routed_container_runtime(
    repo_root: &Path,
    container_name: &str,
) -> Result<crate::runner::container_runtime_prep::ContainerTaskActivation, RunnerError> {
    activate_routed_container_runtime_with(
        repo_root,
        container_name,
        |repo_root, container_name| {
            load_container_policy(repo_root, Some(container_name))
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        },
        |repo_root, policy, request, _plan| {
            activate_container_runtime_for_task(repo_root, policy, request)
        },
    )
}

fn activate_routed_container_runtime_with(
    repo_root: &Path,
    container_name: &str,
    load_policy: impl FnOnce(
        &Path,
        &str,
    ) -> Result<effigy_containers::EffectiveContainerPolicy, RunnerError>,
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
    let policy = load_policy(repo_root, container_name)?;
    let session_context = current_runtime_session_context();
    let plan = standard_runtime_activation_plan(
        repo_root,
        policy.name.as_str(),
        Some(container_name.to_owned()),
        session_context,
    );
    activate(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: plan.request.container_name.as_deref(),
            repo_override: plan.request.repo_override.clone(),
            route: plan.route,
            session_context,
        },
        &plan,
    )
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

fn should_stay_in_workspace_shell(
    output_json: bool,
    task: &effigy_manifest::ManifestTask,
    container_binding: &ContainerExecutionBinding,
) -> bool {
    if output_json
        || inside_container_handoff()
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
) -> Result<String, RunnerError> {
    let repo_root = &selection.catalog.catalog_root;
    let policy = binding_resolution
        .effective_policy(repo_root)?
        .ok_or_else(|| RunnerError::task_invocation("missing inline workspace container policy"))?;
    let working_dir = binding_resolution
        .exec_working_dir(repo_root)?
        .ok_or_else(|| RunnerError::task_invocation("missing inline workspace exec working dir"))?;
    let _ = activate_inline_workspace_container_runtime(repo_root, &policy)?;

    let exec_result = if preflight.output_json {
        let output = capture_routed_task_container_exec_with_policy(
            RoutedTaskExecRequest {
                repo_root,
                invocation_cwd: &preflight.invocation_cwd,
                selector: &preflight.selector,
                task_args: &preflight.runtime_args_exec.passthrough,
                service: policy.primary_service.as_str(),
                command: context.command(),
                task_env: Some(&context.selection.task.env),
                secret_env: secret_ref,
            },
            &policy,
            &working_dir,
        );
        let _ = run_compose_capture(
            repo_root,
            &policy,
            &compose_args(&policy, ["down", "--remove-orphans"]),
            "docker compose down",
        );
        let output = output?;
        let stdout =
            redact_task_secret_values(&String::from_utf8_lossy(&output.stdout), secret_ref);
        let stderr =
            redact_task_secret_values(&String::from_utf8_lossy(&output.stderr), secret_ref);
        let rendered = render::render_task_command_json(
            &preflight.selector.task_name,
            &preflight.selector,
            context.repo_for_task(),
            context.command(),
            output.status.code(),
            &stdout,
            &stderr,
        )?;
        if output.status.success() {
            Ok(rendered)
        } else {
            Err(RunnerError::CommandJsonFailure { rendered })
        }
    } else {
        let result = run_routed_task_container_exec_with_policy(
            RoutedTaskExecRequest {
                repo_root,
                invocation_cwd: &preflight.invocation_cwd,
                selector: &preflight.selector,
                task_args: &preflight.runtime_args_exec.passthrough,
                service: policy.primary_service.as_str(),
                command: context.command(),
                task_env: Some(&context.selection.task.env),
                secret_env: secret_ref,
            },
            &policy,
            &working_dir,
        );
        let _ = run_compose_capture(
            repo_root,
            &policy,
            &compose_args(&policy, ["down", "--remove-orphans"]),
            "docker compose down",
        );
        result.map(|_| {
            if preflight.runtime_args_raw.verbose_root {
                context.render_resolution_trace()
            } else {
                String::new()
            }
        })
    };

    exec_result
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
        },
        &plan,
    )
}

pub(in crate::runner) fn resolve_env_schema_if_present(
    catalog_root: &Path,
    runtime_override: Option<&Path>,
    config: Option<&ManifestEnvSchemaConfig>,
) -> Result<Option<ResolvedEnv>, RunnerError> {
    let shared_config = SchemaSupportConfig {
        config_schema: config.and_then(|c| c.schema.as_deref()),
        config_enabled: config.and_then(|c| c.enabled),
        config_exec_timeout_secs: config.and_then(|c| c.exec_timeout),
    };
    shared_resolve_env_schema(catalog_root, shared_config, runtime_override)
        .map_err(map_schema_support_error)
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

#[cfg(test)]
mod tests {
    use super::{
        activate_inline_workspace_container_runtime_with, activate_routed_container_runtime_with,
        should_stay_in_workspace_shell, ContainerExecutionBinding,
    };
    use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_NAME as CONTAINER_HANDOFF_ENV;
    use crate::runner::container_runtime_prep::ContainerTaskActivation;
    use crate::runner::execute::workspace_seeded::render_workspace_seeded_task_command;
    use crate::runner::runtime_session_context::LeaseRefreshPolicy;
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{ManifestManagedRun, ManifestTask, ManifestTaskRunIn};
    use effigy_runtime_plan::RuntimeLeasePolicy;
    use std::env;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Serializes tests that read or mutate `CONTAINER_HANDOFF_ENV`.
    /// `should_stay_in_workspace_shell` reads the env directly via
    /// `std::env::var_os`, so tests touching that env must not run in
    /// parallel with each other or with tests that read the same env.
    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct EnvGuard {
        key: &'static str,
        old: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &'static str) -> Self {
            let old = env::var_os(key);
            unsafe {
                env::set_var(key, value);
            }
            Self { key, old }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.old.take() {
                Some(value) => unsafe {
                    env::set_var(self.key, value);
                },
                None => unsafe {
                    env::remove_var(self.key);
                },
            }
        }
    }

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
    fn workspace_seeded_task_command_preserves_passthrough_args() {
        let rendered =
            render_workspace_seeded_task_command("seed", &["--".to_owned(), "--force".to_owned()]);

        assert!(
            rendered.ends_with("effigy 'seed' '--' '--force'"),
            "got: {rendered}"
        );
    }

    #[test]
    fn stay_in_shell_requires_explicit_task_opt_in() {
        let _lock = env_lock();
        // Defensive: ensure the env isn't set if a stale prior process left
        // it behind. The lock prevents concurrent test mutation.
        let _clear = unsafe {
            let prior = env::var_os(CONTAINER_HANDOFF_ENV);
            env::remove_var(CONTAINER_HANDOFF_ENV);
            EnvRestore {
                key: CONTAINER_HANDOFF_ENV,
                value: prior,
            }
        };

        assert!(should_stay_in_workspace_shell(
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
        let _lock = env_lock();
        let _env = EnvGuard::set(CONTAINER_HANDOFF_ENV, "1");

        assert!(!should_stay_in_workspace_shell(
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
                    name: "web".to_owned(),
                    driver: effigy_manifest::ManifestContainerDriver::Colima,
                    startup: effigy_manifest::ManifestContainerStartup::Detached,
                    profile: "effigy".to_owned(),
                    compose_source: EffectiveComposeSource::Direct,
                    compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
                    compose_file_display: "docker-compose.yml".to_owned(),
                    managed_volumes: vec![],
                    shared_services: vec![],
                    project_name: "demo-web".to_owned(),
                    primary_service: "app".to_owned(),
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
                    on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
                    shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
                    detach_timeout_secs: 10,
                    host_processes: Vec::new(),
                })
            },
            |repo_root, _policy, request, plan| {
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
                    system_was_running: false,
                    refreshed_host_container_lease: true,
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
        assert!(activation.refreshed_host_container_lease);
    }

    #[test]
    fn inline_workspace_activation_skips_host_container_lease_refresh() {
        let repo_root = Path::new("/tmp/demo-repo");
        let policy = EffectiveContainerPolicy {
            name: "dev__app".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("/tmp/docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: vec![],
            project_name: "demo-web".to_owned(),
            primary_service: "app".to_owned(),
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
                    system_was_running: false,
                    refreshed_host_container_lease: false,
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
        assert!(!activation.refreshed_host_container_lease);
    }

    struct EnvRestore {
        key: &'static str,
        value: Option<std::ffi::OsString>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.value.take() {
                Some(value) => unsafe {
                    env::set_var(self.key, value);
                },
                None => unsafe {
                    env::remove_var(self.key);
                },
            }
        }
    }
}
