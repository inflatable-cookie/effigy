//! Thin adapter — the resolver lives in `effigy-env::schema_support`.
//!
//! Moved out in batch 241 so `effigy-managed` can depend on the
//! resolver through a shared crate rather than reaching back into the
//! runner binary. The adapter below maps the shared
//! `SchemaSupportError` into `RunnerError` variants so runner call
//! sites continue to see the same error shapes.

use std::path::Path;

use effigy_env::resolver::ResolvedEnv;
use effigy_env::schema_support::{
    resolve_catalog_env_schema as shared_resolve, SchemaSupportConfig, SchemaSupportError,
};

use crate::runner::error::RunnerError;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;

pub(in crate::runner) fn resolve_catalog_env_schema(
    catalog_root: &Path,
    config: Option<&ManifestEnvSchemaConfig>,
    runtime_override: Option<&Path>,
) -> Result<Option<ResolvedEnv>, RunnerError> {
    let shared_config = SchemaSupportConfig {
        config_schema: config.and_then(|c| c.schema.as_deref()),
        config_enabled: config.and_then(|c| c.enabled),
        config_exec_timeout_secs: config.and_then(|c| c.exec_timeout),
    };
    shared_resolve(catalog_root, shared_config, runtime_override).map_err(map_error)
}

fn map_error(error: SchemaSupportError) -> RunnerError {
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
