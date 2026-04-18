use std::path::Path;

use super::super::super::cache::ops::check_task_cache;
use super::super::super::exec_command::{
    capture_routed_task_container_exec, run_routed_task_container_exec,
};
use super::super::super::locking::io::acquire_scopes;
use super::super::context::ExecutionTaskContext;
use super::super::preflight::ExecutionPreflight;
use super::super::routing::{route_standard_task_execution, routed_container_target};
use super::{super::cache_hit, super::json_payload, super::process_run, command};
use crate::runner::error::RunnerError;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use effigy_containers::exec::colima_is_running;
use effigy_containers::load_container_policy;
use effigy_env::resolver::ResolvedEnv;
use effigy_env::schema_support::{
    resolve_catalog_env_schema as shared_resolve_env_schema, SchemaSupportConfig,
    SchemaSupportError,
};
use effigy_env::secret::SecretString;
use effigy_manifest::TaskSelection;

pub(super) fn run_standard_task(
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
            &preflight.selector.task_name,
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
        return cache_hit::render_cache_hit_output(
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

    let routed = route_standard_task_execution(
        &preflight.selector.task_name,
        selection.task,
        selection.catalog.manifest.containers.as_ref(),
        |container_name| {
            let policy =
                load_container_policy(&selection.catalog.catalog_root, Some(container_name))?;
            colima_is_running(&policy, &selection.catalog.catalog_root).map_err(Into::into)
        },
    )?;

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
            let rendered = json_payload::render_task_command_json(
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

    process_run::run_task_process(
        preflight.output_json,
        preflight.runtime_args_raw.verbose_root,
        &context,
        secret_ref,
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
