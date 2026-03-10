use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use super::super::super::cache::ops::check_task_cache;
use super::super::super::locking::io::acquire_scopes;
use super::super::super::locking::model::LockScope;
use super::super::context::ExecutionTaskContext;
use super::super::preflight::ExecutionPreflight;
use super::{super::cache_hit, super::process_run, command};
use crate::env_schema::resolver::ResolvedEnv;
use crate::runner::error::RunnerError;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use crate::runner::model::catalog::TaskSelection;

pub(super) fn run_standard_task(
    preflight: &ExecutionPreflight,
    selection: &TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let env_schema_resolved = resolve_env_schema_if_present(
        &selection.catalog.catalog_root,
        selection.catalog.manifest.env_schema.as_ref(),
    )?;

    let context = ExecutionTaskContext::new(
        preflight,
        selection,
        command::build_task_command(preflight, selection, &env_schema_resolved)?,
    );

    let _lock_guards = acquire_scopes(
        &preflight.resolved.resolved_root,
        &[
            LockScope::Workspace,
            LockScope::Task(preflight.selector.task_name.clone()),
        ],
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

    let secret_pairs: Option<Vec<(&str, &crate::env_schema::secret::SecretString)>> =
        env_schema_resolved.as_ref().map(|r| r.secret_env());
    let secret_ref = secret_pairs.as_deref();

    process_run::run_task_process(
        preflight.output_json,
        preflight.runtime_args_raw.verbose_root,
        &context,
        secret_ref,
    )
}

fn resolve_env_schema_if_present(
    catalog_root: &Path,
    config: Option<&ManifestEnvSchemaConfig>,
) -> Result<Option<ResolvedEnv>, RunnerError> {
    // If explicitly disabled, skip entirely.
    if let Some(cfg) = config {
        if cfg.enabled == Some(false) {
            return Ok(None);
        }
    }

    // Determine schema path: custom path from config, or default `.env.schema`.
    let schema_path = match config.and_then(|c| c.schema.as_deref()) {
        Some(custom) => catalog_root.join(custom),
        None => catalog_root.join(".env.schema"),
    };

    // If not explicitly enabled, only proceed if the schema file exists (auto-detect).
    // If explicitly enabled with a custom path that doesn't exist, that's an error.
    if !schema_path.is_file() {
        let explicitly_enabled = config.is_some_and(|c| c.enabled == Some(true));
        if explicitly_enabled {
            return Err(RunnerError::task_invocation(format!(
                "env_schema enabled but schema file not found: {}",
                schema_path.display()
            )));
        }
        return Ok(None);
    }

    let dotenv_path = catalog_root.join(".env");
    let dotenv_overrides = if dotenv_path.is_file() {
        let content = std::fs::read_to_string(&dotenv_path).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to read {}: {error}",
                dotenv_path.display()
            ))
        })?;
        crate::runner::util::parse_dotenv_entries(&content)
    } else {
        BTreeMap::new()
    };

    let exec_timeout = config.and_then(|c| c.exec_timeout).unwrap_or(30);

    let resolved = crate::env_schema::load_and_resolve(
        &schema_path,
        &dotenv_overrides,
        Duration::from_secs(exec_timeout),
        catalog_root,
    )?;

    Ok(Some(resolved))
}
