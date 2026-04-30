use std::path::Path;

use super::super::super::cache::ops::check_task_cache;
use super::super::super::exec_command::{
    capture_routed_task_container_exec, capture_routed_task_container_exec_with_policy,
    run_routed_task_container_exec, run_routed_task_container_exec_with_policy,
};
use super::super::super::locking::io::acquire_scopes;
use super::super::super::system_command::{
    is_primary_service_running, run_workspace_seeded_session,
};
use super::super::api::{resolve_container_execution_binding, ContainerExecutionBinding};
use super::super::context::ExecutionTaskContext;
use super::super::planning::ExecutionPreflight;
use super::super::routing::{
    route_standard_task_execution, routed_container_target, routed_not_running_container,
    RoutedTaskExecution,
};
use super::{super::process_run, command};
use crate::runner::error::RunnerError;
use crate::runner::execute::nested;
use crate::runner::execute::render;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use effigy_containers::compose::{compose_args, compose_up_args};
use effigy_containers::exec::{ensure_colima_running, run_docker_capture};
use effigy_containers::{
    load_container_policy, validate_compose_backend_runtime, validate_container_policy,
};
use effigy_env::resolver::ResolvedEnv;
use effigy_env::schema_support::{
    resolve_catalog_env_schema as shared_resolve_env_schema, SchemaSupportConfig,
    SchemaSupportError,
};
use effigy_env::secret::SecretString;
use effigy_manifest::TaskSelection;

const CONTAINER_HANDOFF_ENV: &str = "EFFIGY_INTERNAL_CONTAINER_HANDOFF";

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

    let container_binding = resolve_container_execution_binding(
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
    if let ContainerExecutionBinding::Inline { .. } = &container_binding {
        return run_inline_workspace_standard_task(
            preflight,
            selection,
            &context,
            secret_ref,
            &container_binding,
        );
    }

    let routed = route_with_running_check(preflight, selection)?;

    let routed = if let Some(container_name) = routed_not_running_container(&routed.decision) {
        ensure_routed_container_up(&selection.catalog.catalog_root, container_name)?;
        let rerouted = route_with_running_check(preflight, selection)?;
        if rerouted.decision.is_not_running() {
            return Err(RunnerError::task_invocation(format!(
                "task `{}` requires container `{}` but it is still not running after auto-up",
                preflight.selector.task_name, container_name
            )));
        }
        rerouted
    } else {
        routed
    };

    if should_stay_in_workspace_shell(preflight.output_json, selection.task, &container_binding) {
        return run_workspace_seeded_session(
            &selection.catalog.catalog_root,
            container_binding.container_name(),
            preflight.runtime_args_raw.repo_override.clone(),
            &render_workspace_seeded_task_command(
                &preflight.selector.task_name,
                &preflight.runtime_args_exec.passthrough,
            ),
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

fn ensure_routed_container_up(repo_root: &Path, container_name: &str) -> Result<(), RunnerError> {
    let policy = load_container_policy(repo_root, Some(container_name))
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let _colima_started = ensure_colima_running(&policy, repo_root)?;
    run_docker_capture(
        repo_root,
        &policy,
        &compose_up_args(&policy),
        "docker compose up",
    )?;
    Ok(())
}

fn should_stay_in_workspace_shell(
    output_json: bool,
    task: &effigy_manifest::ManifestTask,
    container_binding: &ContainerExecutionBinding,
) -> bool {
    if output_json
        || std::env::var_os(CONTAINER_HANDOFF_ENV).is_some()
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

fn render_workspace_seeded_task_command(task_name: &str, args: &[String]) -> String {
    let mut rendered = format!("effigy {}", effigy_core::shell::shell_quote(task_name));
    let rendered_args = crate::runner::util::render_passthrough_args(args);
    if !rendered_args.is_empty() {
        rendered.push(' ');
        rendered.push_str(&rendered_args);
    }
    rendered
}

fn run_inline_workspace_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
    context: &ExecutionTaskContext<'_>,
    secret_ref: Option<&[(&str, &SecretString)]>,
    container_binding: &ContainerExecutionBinding,
) -> Result<String, RunnerError> {
    let repo_root = &selection.catalog.catalog_root;
    let policy = container_binding
        .load_effective_policy(repo_root)?
        .ok_or_else(|| RunnerError::task_invocation("missing inline workspace container policy"))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_compose_backend_runtime(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let working_dir = container_binding
        .exec_working_dir(repo_root)?
        .ok_or_else(|| RunnerError::task_invocation("missing inline workspace exec working dir"))?;

    let _colima_started = ensure_colima_running(&policy, repo_root)?;
    run_docker_capture(
        repo_root,
        &policy,
        &effigy_containers::compose::compose_up_args(&policy),
        "docker compose up",
    )?;

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
        render_workspace_seeded_task_command, should_stay_in_workspace_shell,
        ContainerExecutionBinding, CONTAINER_HANDOFF_ENV,
    };
    use effigy_manifest::{ManifestManagedRun, ManifestTask, ManifestTaskRunIn};
    use std::env;
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
