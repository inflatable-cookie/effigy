//! Higher-level entry point for loading an env schema at a catalog
//! root, applying `.env` overrides, and returning a `ResolvedEnv`.
//!
//! Moved out of `src/runner/env_schema_support.rs` in batch 241 so
//! `effigy-managed` and the runner's `execute::pipeline::standard`
//! flow can both reach it through `effigy-env` rather than via a
//! runner-internal path. The runner keeps a thin adapter that maps
//! `SchemaSupportError` into `RunnerError`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::dotenv::parse_dotenv_entries;
use crate::error::EnvSchemaError;
use crate::resolver::ResolvedEnv;

/// Inputs required to resolve the env schema for a catalog.
///
/// The three `config_*` fields mirror the subset of
/// `ManifestEnvSchemaConfig` that the resolver actually reads —
/// passing the primitives directly keeps `effigy-env` free of an
/// `effigy-manifest` dependency.
#[derive(Debug, Clone, Default)]
pub struct SchemaSupportConfig<'a> {
    pub config_schema: Option<&'a str>,
    pub config_enabled: Option<bool>,
    pub config_exec_timeout_secs: Option<u64>,
}

#[derive(Debug)]
pub enum SchemaSupportError {
    InvalidConfig(String),
    SchemaFileMissing(PathBuf),
    DotenvRead {
        path: PathBuf,
        error: std::io::Error,
    },
    Schema(EnvSchemaError),
}

impl std::fmt::Display for SchemaSupportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "{message}"),
            Self::SchemaFileMissing(path) => {
                write!(f, "env schema file not found: {}", path.display())
            }
            Self::DotenvRead { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
            Self::Schema(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SchemaSupportError {}

impl From<EnvSchemaError> for SchemaSupportError {
    fn from(value: EnvSchemaError) -> Self {
        Self::Schema(value)
    }
}

pub fn resolve_catalog_env_schema(
    catalog_root: &Path,
    config: SchemaSupportConfig<'_>,
    runtime_override: Option<&Path>,
) -> Result<Option<ResolvedEnv>, SchemaSupportError> {
    validate_schema_config(&config)?;

    if runtime_override.is_none() && config.config_enabled == Some(false) {
        return Ok(None);
    }

    let schema_path = runtime_override
        .map(|path| resolve_schema_path(catalog_root, path))
        .or_else(|| {
            config
                .config_schema
                .map(|path| resolve_schema_path(catalog_root, Path::new(path)))
        })
        .unwrap_or_else(|| catalog_root.join(".env.schema"));

    if !schema_path.is_file() {
        let must_exist = runtime_override.is_some() || config.config_enabled == Some(true);
        if must_exist {
            return Err(SchemaSupportError::SchemaFileMissing(schema_path));
        }
        return Ok(None);
    }

    let dotenv_path = catalog_root.join(".env");
    let dotenv_overrides = if dotenv_path.is_file() {
        let content = std::fs::read_to_string(&dotenv_path).map_err(|error| {
            SchemaSupportError::DotenvRead {
                path: dotenv_path.clone(),
                error,
            }
        })?;
        parse_dotenv_entries(&content)
    } else {
        BTreeMap::new()
    };

    let exec_timeout = config.config_exec_timeout_secs.unwrap_or(30);

    Ok(crate::load_and_resolve(
        &schema_path,
        &dotenv_overrides,
        Duration::from_secs(exec_timeout),
        catalog_root,
    )
    .map(Some)?)
}

fn validate_schema_config(config: &SchemaSupportConfig<'_>) -> Result<(), SchemaSupportError> {
    if config
        .config_schema
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(SchemaSupportError::InvalidConfig(
            "invalid `[env_schema].schema`: value cannot be empty".to_owned(),
        ));
    }

    if config.config_exec_timeout_secs == Some(0) {
        return Err(SchemaSupportError::InvalidConfig(
            "invalid `[env_schema].exec_timeout`: value must be at least 1 second".to_owned(),
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
