use std::path::{Path, PathBuf};

#[path = "env_schema/ast.rs"]
pub mod ast;
#[path = "env_schema/error.rs"]
pub mod error;
#[path = "env_schema/exec.rs"]
mod exec;
#[path = "env_schema/parser.rs"]
mod parser;
#[path = "env_schema/resolver.rs"]
pub mod resolver;
#[path = "env_schema/secret.rs"]
pub mod secret;
#[path = "env_schema/validator.rs"]
pub mod validator;

pub use ast::{EnvSchema, EnvSchemaEntry, EnvType, EnvValueExpr};
pub use error::{EnvSchemaError, ValidationError};
pub use resolver::{ResolutionContext, ResolvedEntry, ResolvedEnv, ResolvedSource};
pub use secret::ResolvedValue as EnvValue;
pub use secret::{ResolvedValue, SecretString};

const DEFAULT_SCHEMA_FILE: &str = ".env.schema";

/// Return the default env-schema path when present in the provided project root.
pub fn detect_schema_path(project_root: &Path) -> Option<PathBuf> {
    let path = project_root.join(DEFAULT_SCHEMA_FILE);
    path.is_file().then_some(path)
}

/// Load and parse a `.env.schema` file.
pub fn load_env_schema(path: &Path) -> Result<EnvSchema, EnvSchemaError> {
    load_schema(path)
}

/// Load the default `.env.schema` from a project root if it exists.
pub fn load_env_schema_if_present(
    project_root: &Path,
) -> Result<Option<EnvSchema>, EnvSchemaError> {
    detect_schema_path(project_root)
        .map(|path| load_env_schema(&path))
        .transpose()
}

/// Load and parse a `.env.schema` file.
pub fn load_schema(path: &Path) -> Result<EnvSchema, EnvSchemaError> {
    let content = std::fs::read_to_string(path).map_err(|error| EnvSchemaError::Io {
        path: path.to_owned(),
        error,
    })?;
    parser::parse_env_schema(&content, path)
}

/// Resolve an already-loaded env schema with the provided environment context.
pub fn resolve_env(
    schema: &EnvSchema,
    context: &ResolutionContext,
) -> Result<ResolvedEnv, EnvSchemaError> {
    resolver::resolve_schema(schema, context)
}

/// Validate resolved values against a loaded env schema.
pub fn validate_env(schema: &EnvSchema, resolved: &ResolvedEnv) -> Vec<ValidationError> {
    validator::validate_resolved_env(schema, resolved)
}

/// Resolve an env schema and fail if validation reports any errors.
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

/// Full pipeline: load schema, resolve values, validate types.
///
/// Returns the resolved environment or the first error encountered.
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
