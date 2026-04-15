use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::runner::error::RunnerError;
use crate::runner::manifest::config_sections::ManifestEnvSchemaConfig;
use effigy_env::resolver::ResolvedEnv;

pub(in crate::runner) fn resolve_catalog_env_schema(
    catalog_root: &Path,
    config: Option<&ManifestEnvSchemaConfig>,
    runtime_override: Option<&Path>,
) -> Result<Option<ResolvedEnv>, RunnerError> {
    validate_env_schema_config(config)?;

    if runtime_override.is_none() && config.is_some_and(|cfg| cfg.enabled == Some(false)) {
        return Ok(None);
    }

    let schema_path = runtime_override
        .map(|path| resolve_schema_path(catalog_root, path))
        .or_else(|| {
            config
                .and_then(|c| c.schema.as_deref())
                .map(|path| resolve_schema_path(catalog_root, Path::new(path)))
        })
        .unwrap_or_else(|| catalog_root.join(".env.schema"));

    if !schema_path.is_file() {
        let must_exist =
            runtime_override.is_some() || config.is_some_and(|c| c.enabled == Some(true));
        if must_exist {
            return Err(RunnerError::task_invocation(format!(
                "env schema file not found: {}",
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

    Ok(effigy_env::load_and_resolve(
        &schema_path,
        &dotenv_overrides,
        Duration::from_secs(exec_timeout),
        catalog_root,
    )
    .map(Some)?)
}

fn validate_env_schema_config(config: Option<&ManifestEnvSchemaConfig>) -> Result<(), RunnerError> {
    let Some(config) = config else {
        return Ok(());
    };

    if config
        .schema
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(RunnerError::task_invocation(
            "invalid `[env_schema].schema`: value cannot be empty",
        ));
    }

    if config.exec_timeout == Some(0) {
        return Err(RunnerError::task_invocation(
            "invalid `[env_schema].exec_timeout`: value must be at least 1 second",
        ));
    }

    Ok(())
}

fn resolve_schema_path(catalog_root: &Path, candidate: &Path) -> PathBuf {
    if candidate.is_absolute() {
        return candidate.to_path_buf();
    }
    catalog_root.join(candidate)
}
