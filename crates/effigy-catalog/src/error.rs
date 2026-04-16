//! Error types for the catalog crate.

use std::path::PathBuf;

/// Errors that can occur during catalog operations.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    /// A referenced catalog fragment was not found in any catalog layer.
    #[error("catalog fragment not found: {name}")]
    FragmentNotFound { name: String },

    /// The service.toml file in a fragment is missing or unreadable.
    #[error("missing service.toml in fragment '{name}'")]
    MissingServiceToml { name: String },

    /// The service.toml file could not be parsed.
    #[error("invalid service.toml in fragment '{name}': {reason}")]
    InvalidServiceToml { name: String, reason: String },

    /// The compose.fragment.yml file is missing.
    #[error("missing compose.fragment.yml in fragment '{name}'")]
    MissingComposeFragment { name: String },

    /// A required parameter was not provided and has no default.
    #[error("missing required parameter '{param}' for service '{service}'")]
    MissingRequiredParam { service: String, param: String },

    /// A parameter value does not match the declared type.
    #[error(
        "type mismatch for parameter '{param}' in service '{service}': \
         expected {expected}, got {actual}"
    )]
    ParamTypeMismatch {
        service: String,
        param: String,
        expected: String,
        actual: String,
    },

    /// Template rendering failed.
    #[error("template render error in fragment '{name}': {reason}")]
    TemplateRenderError { name: String, reason: String },

    /// The assembled compose output could not be written.
    #[error("failed to write compose file to {path}: {reason}")]
    WriteError { path: PathBuf, reason: String },

    /// A config variant was requested but does not exist in the fragment.
    #[error(
        "config variant '{variant}' not found in fragment '{name}' \
         (available: {available:?})"
    )]
    VariantNotFound {
        name: String,
        variant: String,
        available: Vec<String>,
    },

    /// A project-local config file was referenced but does not exist.
    #[error("config file not found: {path}")]
    ConfigFileNotFound { path: PathBuf },

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
