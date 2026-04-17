use std::path::Path;

use super::super::super::cache::ops::check_task_cache;
use super::super::super::locking::io::acquire_scopes;
use super::super::context::ExecutionTaskContext;
use super::super::preflight::ExecutionPreflight;
use super::{super::cache_hit, super::process_run, command};
use crate::runner::error::RunnerError;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use effigy_env::resolver::ResolvedEnv;
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
    crate::runner::env_schema_support::resolve_catalog_env_schema(
        catalog_root,
        config,
        runtime_override,
    )
}
