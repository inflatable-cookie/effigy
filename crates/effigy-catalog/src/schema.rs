//! Service fragment schema — parsed from `service.toml`.
//!
//! Each catalog fragment declares its parameter interface, capabilities,
//! volume declarations, and dependency expectations in a `service.toml` file.

use indexmap::IndexMap;
use serde::Deserialize;

/// Top-level schema for a service fragment's `service.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceSchema {
    /// Service identity.
    pub service: ServiceIdentity,

    /// Parameter declarations, keyed by parameter name.
    #[serde(default)]
    pub params: IndexMap<String, ParamSchema>,

    /// What this service can do (exec target, shell path, etc.).
    #[serde(default)]
    pub capabilities: Capabilities,

    /// Named volume declarations.
    #[serde(default)]
    pub volumes: IndexMap<String, VolumeSchema>,

    /// Default ports exposed by this service.
    #[serde(default)]
    pub ports: PortsSchema,

    /// Dependency expectations for other services in the stack.
    #[serde(default)]
    pub depends_on: DependsOnSchema,
}

/// Service identity metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceIdentity {
    /// Fragment name (must match directory name).
    pub name: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,
}

/// Schema for a single parameter.
#[derive(Debug, Clone, Deserialize)]
pub struct ParamSchema {
    /// Parameter type.
    #[serde(rename = "type")]
    pub param_type: ParamType,

    /// Default value. If absent, the parameter is required.
    #[serde(default)]
    pub default: Option<toml::Value>,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,
}

impl ParamSchema {
    /// Whether this parameter is required (no default value).
    pub fn is_required(&self) -> bool {
        self.default.is_none()
    }
}

/// Supported parameter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    String,
    List,
    Bool,
    Integer,
}

impl std::fmt::Display for ParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::List => write!(f, "list"),
            Self::Bool => write!(f, "bool"),
            Self::Integer => write!(f, "integer"),
        }
    }
}

/// Service capabilities.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Capabilities {
    /// Whether this service is a valid target for `effigy exec`.
    #[serde(default)]
    pub exec_target: bool,

    /// Default shell for interactive exec (e.g., "/bin/bash").
    #[serde(default)]
    pub shell: Option<String>,

    /// Whether Effigy should apply the workspace host-integration mounts for
    /// git and SSH when this catalog is the primary workspace service.
    #[serde(default)]
    pub workspace_host_integration: bool,

    /// Whether the catalog image's entrypoint installs a mounted mkcert root CA
    /// into the system trust store on container startup.
    #[serde(default)]
    pub installs_mkcert_ca: bool,

    /// Optional DNS label Effigy should publish as a per-service loopback alias
    /// when this catalog appears in a container stack.
    #[serde(default)]
    pub loopback_alias_label: Option<String>,

    /// Container port that backs the published loopback alias. When absent, no
    /// alias contract is emitted even if the catalog exposes normal ports.
    #[serde(default)]
    pub loopback_alias_port: Option<u16>,

    /// Whether this catalog is eligible for the bounded `shared = true`
    /// backing-service path.
    #[serde(default)]
    pub shared_service: bool,

    /// Environment variable names that should receive the shared service host
    /// value when this catalog is reused through the bounded shared-service
    /// path.
    #[serde(default)]
    pub shared_host_env_vars: Vec<String>,

    /// Environment variable names that should receive the shared service port
    /// value when this catalog is reused through the bounded shared-service
    /// path.
    #[serde(default)]
    pub shared_port_env_vars: Vec<String>,
}

/// Volume declaration for a service.
#[derive(Debug, Clone, Deserialize)]
pub struct VolumeSchema {
    /// Mount path inside the container.
    pub mount: String,

    /// Whether this is a named Docker volume (vs. a bind mount).
    #[serde(default = "default_true")]
    pub named: bool,

    /// Whether this volume should survive `container reset --keep-data`.
    #[serde(default = "default_true")]
    pub persist: bool,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,
}

/// Port declarations.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PortsSchema {
    /// Default ports exposed by this service.
    #[serde(default)]
    pub default: Vec<u16>,

    /// Description of what the ports are for.
    #[serde(default)]
    pub description: String,
}

/// Dependency expectations.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DependsOnSchema {
    /// Catalog service types this fragment can reference.
    /// The assembly engine only populates context for services actually
    /// present in the stack.
    #[serde(default)]
    pub optional: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl ServiceSchema {
    /// Parse a `service.toml` from its string contents.
    pub fn parse(contents: &str, fragment_name: &str) -> Result<Self, crate::CatalogError> {
        toml::from_str(contents).map_err(|e| crate::CatalogError::InvalidServiceToml {
            name: fragment_name.to_string(),
            reason: e.to_string(),
        })
    }
}

#[cfg(test)]
#[path = "schema/tests.rs"]
mod tests;
