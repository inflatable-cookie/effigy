use std::path::Path;

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
pub use secret::{ResolvedValue, SecretString};

/// Load and parse a `.env.schema` file.
pub fn load_schema(path: &Path) -> Result<EnvSchema, EnvSchemaError> {
    let content = std::fs::read_to_string(path).map_err(|error| EnvSchemaError::Io {
        path: path.to_owned(),
        error,
    })?;
    parser::parse_env_schema(&content, path)
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
    let schema = load_schema(schema_path)?;
    let context = ResolutionContext {
        process_env: collect_process_env(),
        dotenv_overrides: dotenv_overrides.clone(),
        exec_timeout,
        project_root: project_root.to_owned(),
    };
    let resolved = resolver::resolve_schema(&schema, &context)?;
    let errors = validator::validate_resolved_env(&schema, &resolved);
    if !errors.is_empty() {
        return Err(EnvSchemaError::Validation { errors });
    }
    Ok(resolved)
}

fn collect_process_env() -> std::collections::BTreeMap<String, String> {
    std::env::vars().collect()
}
