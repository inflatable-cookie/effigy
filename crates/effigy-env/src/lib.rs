use std::path::{Path, PathBuf};

pub mod ast;
pub mod error;
mod exec;
mod parser;
pub mod resolver;
pub mod secret;
pub mod validator;

pub use ast::{EnvSchema, EnvSchemaEntry, EnvType, EnvValueExpr};
pub use error::{EnvSchemaError, ValidationError};
pub use resolver::{ResolutionContext, ResolvedEntry, ResolvedEnv, ResolvedSource};
pub use secret::ResolvedValue as EnvValue;
pub use secret::{ResolvedValue, SecretString};

const DEFAULT_SCHEMA_FILE: &str = ".env.schema";

pub fn detect_schema_path(project_root: &Path) -> Option<PathBuf> {
    let path = project_root.join(DEFAULT_SCHEMA_FILE);
    path.is_file().then_some(path)
}

pub fn load_env_schema(path: &Path) -> Result<EnvSchema, EnvSchemaError> {
    load_schema(path)
}

pub fn load_env_schema_if_present(
    project_root: &Path,
) -> Result<Option<EnvSchema>, EnvSchemaError> {
    detect_schema_path(project_root)
        .map(|path| load_env_schema(&path))
        .transpose()
}

pub fn load_schema(path: &Path) -> Result<EnvSchema, EnvSchemaError> {
    let content = std::fs::read_to_string(path).map_err(|error| EnvSchemaError::Io {
        path: path.to_owned(),
        error,
    })?;
    parser::parse_env_schema(&content, path)
}

pub fn resolve_env(
    schema: &EnvSchema,
    context: &ResolutionContext,
) -> Result<ResolvedEnv, EnvSchemaError> {
    resolver::resolve_schema(schema, context)
}

pub fn validate_env(schema: &EnvSchema, resolved: &ResolvedEnv) -> Vec<ValidationError> {
    validator::validate_resolved_env(schema, resolved)
}

pub fn resolve_and_validate_env(
    schema: &EnvSchema,
    context: &ResolutionContext,
) -> Result<ResolvedEnv, EnvSchemaError> {
    let resolved = resolve_env(schema, context)?;
    let errors = validate_env(schema, &resolved);
    if !errors.is_empty() {
        return Err(EnvSchemaError::Validation { errors });
    }
    Ok(resolved)
}

pub fn load_and_resolve(
    schema_path: &Path,
    dotenv_overrides: &std::collections::BTreeMap<String, String>,
    exec_timeout: std::time::Duration,
    project_root: &Path,
) -> Result<ResolvedEnv, EnvSchemaError> {
    let schema = load_env_schema(schema_path)?;
    let context = ResolutionContext {
        process_env: collect_process_env(),
        dotenv_overrides: dotenv_overrides.clone(),
        exec_timeout,
        project_root: project_root.to_owned(),
    };
    resolve_and_validate_env(&schema, &context)
}

fn collect_process_env() -> std::collections::BTreeMap<String, String> {
    std::env::vars().collect()
}
