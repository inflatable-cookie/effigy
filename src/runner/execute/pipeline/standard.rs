use std::path::Path;

use super::super::super::cache::ops::check_task_cache;
use super::super::super::exec_command::{
    capture_routed_task_container_exec, capture_routed_task_container_exec_with_policy,
    run_routed_task_container_exec, run_routed_task_container_exec_with_policy,
};
use super::super::super::locking::io::acquire_scopes;
use super::super::super::system_command::is_primary_service_running;
use super::super::api::{
    resolve_execution_binding_resolution, ContainerExecutionBinding, ExecutionBindingResolution,
};
use super::super::context::ExecutionTaskContext;
use super::super::planning::ExecutionPreflight;
use super::super::routing::{
    route_standard_task_execution, routed_container_target, routed_not_running_container,
    RoutedTaskExecution,
};
use super::{super::process_run, command};
use crate::runner::container_runtime_prep::{
    activate_container_runtime_for_task, ActivationRequest,
};
use crate::runner::error::RunnerError;
use crate::runner::execute::nested;
use crate::runner::execute::render;
use crate::runner::execute::workspace_seeded::{
    inside_container_handoff, run_workspace_seeded_task_session,
};
use crate::runner::host_container_lease::emit_host_container_lease_notice;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use crate::runner::runtime_session_context::{
    current_runtime_session_context, LeaseRefreshPolicy, RuntimeSessionContext,
};
use effigy_containers::compose::compose_args;
use effigy_containers::exec::run_docker_capture;
use effigy_containers::load_container_policy;
use effigy_env::resolver::ResolvedEnv;
use effigy_env::schema_support::{
    resolve_catalog_env_schema as shared_resolve_env_schema, SchemaSupportConfig,
    SchemaSupportError,
};
use effigy_env::secret::SecretString;
use effigy_manifest::TaskSelection;

pub(in crate::runner) fn run_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let env_schema_resolved = resolve_env_schema_if_present(
        &selection.catalog.catalog_root,
        preflight.runtime_args_raw.env_schema_override.as_deref(),
        selection.catalog.manifest.env_schema.as_ref(),
    )?;

    let context = ExecutionTaskContext::new(
        preflight,
        selection,
        command::build_task_command(preflight, selection, &env_schema_resolved)?,
    );

    let _lock_guards = acquire_scopes(
        &preflight.resolved.resolved_root,
        &[crate::runner::manifest::task_lock_scope(
            selection.task,
            &preflight.selector,
        )],
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
        return render::render_cache_hit_output(
            preflight.output_json,
            preflight.runtime_args_raw.verbose_root,
            &context,
            &cache_check.reason,
            &cache_check.fingerprint,
        );
    }

    let secret_pairs: Option<Vec<(&str, &SecretString)>> =
        env_schema_resolved.as_ref().map(|r| r.secret_env());
    let secret_ref = secret_pairs.as_deref();

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
        "standard task execution",
    )?;
    let container_binding = binding_resolution.binding();
    if let ContainerExecutionBinding::Inline { .. } = container_binding {
        return run_inline_workspace_standard_task(
            preflight,
            selection,
            &context,
            secret_ref,
            &binding_resolution,
        );
    }

    let routed = route_with_running_check(preflight, selection)?;

    let mut task_activation = None;
    let routed = if let Some(container_name) = routed_not_running_container(&routed.decision) {
        task_activation = Some(activate_routed_container_runtime(
            &selection.catalog.catalog_root,
            container_name,
        )?);
        let rerouted = route_with_running_check(preflight, selection)?;
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
                &selection.catalog.catalog_root,
                container_name,
            )?);
        }
        routed
    };

    if should_stay_in_workspace_shell(preflight.output_json, selection.task, container_binding) {
        return run_workspace_seeded_task_session(
            &selection.catalog.catalog_root,
            container_binding,
            preflight.runtime_args_raw.repo_override.clone(),
            &preflight.selector.task_name,
            &preflight.runtime_args_exec.passthrough,
            Some(current_runtime_session_context().public_workspace_cleanup),
        );
    }

    if let Some((container, service)) = routed_container_target(&routed.decision) {
        if preflight.output_json {
            let output = capture_routed_task_container_exec(
                &selection.catalog.catalog_root,
                &preflight.invocation_cwd,
                &preflight.selector,
                &preflight.runtime_args_exec.passthrough,
                container,
                service,
                context.command(),
                secret_ref,
            )?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
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
                return Ok(rendered);
            }
            return Err(RunnerError::CommandJsonFailure { rendered });
        }

        run_routed_task_container_exec(
            &selection.catalog.catalog_root,
            &preflight.invocation_cwd,
            &preflight.selector,
            &preflight.runtime_args_exec.passthrough,
            container,
            service,
            context.command(),
            secret_ref,
        )?;
        if task_activation.is_some_and(|activation| activation.refreshed_host_container_lease) {
            emit_host_container_lease_notice(container);
        }
        if preflight.runtime_args_raw.verbose_root {
            return Ok(context.render_resolution_trace());
        }
        return Ok(String::new());
    }

    if let Some(output) = nested::maybe_run_in_process_sequence(
        preflight,
        selection,
        &context,
        &env_schema_resolved,
        secret_ref,
    )? {
        return Ok(output);
    }

    process_run::run_task_process(
        preflight.output_json,
        preflight.runtime_args_raw.verbose_root,
        &context,
        secret_ref,
    )
}

fn route_with_running_check(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<RoutedTaskExecution, RunnerError> {
    route_standard_task_execution(
        &preflight.selector.task_name,
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.task,
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        |container_name| {
            let policy =
                load_container_policy(&selection.catalog.catalog_root, Some(container_name))?;
            is_primary_service_running(&selection.catalog.catalog_root, &policy)
        },
    )
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
        |repo_root, policy, request| {
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
    ) -> Result<
        crate::runner::container_runtime_prep::ContainerTaskActivation,
        RunnerError,
    >,
) -> Result<crate::runner::container_runtime_prep::ContainerTaskActivation, RunnerError> {
    let policy = load_policy(repo_root, container_name)?;
    activate(
        repo_root,
        &policy,
        ActivationRequest {
            container_name: Some(container_name),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: current_runtime_session_context(),
        },
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
            repo_root,
            &preflight.invocation_cwd,
            &preflight.selector,
            &preflight.runtime_args_exec.passthrough,
            &policy,
            &working_dir,
            policy.primary_service.as_str(),
            context.command(),
            secret_ref,
        );
        let _ = run_docker_capture(
            repo_root,
            &policy,
            &compose_args(&policy, ["down", "--remove-orphans"]),
            "docker compose down",
        );
        let output = output?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
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
            repo_root,
            &preflight.invocation_cwd,
            &preflight.selector,
            &preflight.runtime_args_exec.passthrough,
            &policy,
            &working_dir,
            policy.primary_service.as_str(),
            context.command(),
            secret_ref,
        );
        let _ = run_docker_capture(
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
        |repo_root, policy, request| {
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
    ) -> Result<
        crate::runner::container_runtime_prep::ContainerTaskActivation,
        RunnerError,
    >,
) -> Result<crate::runner::container_runtime_prep::ContainerTaskActivation, RunnerError> {
    activate(
        repo_root,
        policy,
        ActivationRequest {
            container_name: Some(policy.name.as_str()),
            repo_override: Some(repo_root.to_path_buf()),
            session_context: RuntimeSessionContext {
                lease_refresh_policy: LeaseRefreshPolicy::SkipRefresh,
                ..current_runtime_session_context()
            },
        },
    )
}

fn resolve_env_schema_if_present(
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

        assert_eq!(rendered, "effigy 'seed' '--' '--force'");
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
                    workspace_user: None,
                    workspace_home: None,
                    on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
                    shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
                    detach_timeout_secs: 10,
                    host_processes: Vec::new(),
                })
            },
            |repo_root, _policy, request| {
                activation_call = Some((
                    repo_root.to_path_buf(),
                    request.container_name.map(str::to_owned),
                    request.repo_override,
                    request.session_context.lease_refresh_policy,
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
            |repo_root, policy, request| {
                activation_call = Some((
                    repo_root.to_path_buf(),
                    policy.name.clone(),
                    request.container_name.map(str::to_owned),
                    request.repo_override,
                    request.session_context.lease_refresh_policy,
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
