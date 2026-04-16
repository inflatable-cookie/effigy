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
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_service_toml() {
        let input = r#"
[service]
name = "test-svc"

[params.version]
type = "string"
default = "1.0"
"#;
        let schema = ServiceSchema::parse(input, "test-svc").unwrap();
        assert_eq!(schema.service.name, "test-svc");
        assert_eq!(schema.params.len(), 1);
        assert_eq!(schema.params["version"].param_type, ParamType::String);
        assert!(!schema.params["version"].is_required());
    }

    #[test]
    fn parse_full_service_toml() {
        let input = r#"
[service]
name = "php-fpm"
description = "PHP-FPM application server"

[params.version]
type = "string"
default = "8.3"
description = "PHP version"

[params.extensions]
type = "list"
default = []
description = "PHP extensions to install"

[params.document_root]
type = "string"
default = "public"

[params.working_dir]
type = "string"
default = "/var/www/html"

[capabilities]
exec_target = true
shell = "/bin/bash"

[volumes.data]
mount = "/var/lib/mysql"
named = true
persist = true
description = "Database storage"

[ports]
default = [9000]
description = "FastCGI port"

[depends_on]
optional = ["mariadb", "postgres", "redis"]
"#;
        let schema = ServiceSchema::parse(input, "php-fpm").unwrap();
        assert_eq!(schema.service.name, "php-fpm");
        assert_eq!(schema.params.len(), 4);
        assert!(schema.capabilities.exec_target);
        assert_eq!(schema.capabilities.shell.as_deref(), Some("/bin/bash"));
        assert_eq!(schema.volumes.len(), 1);
        assert!(schema.volumes["data"].persist);
        assert_eq!(schema.ports.default, vec![9000]);
        assert_eq!(schema.depends_on.optional.len(), 3);
    }

    #[test]
    fn required_param_detected() {
        let input = r#"
[service]
name = "test"

[params.api_key]
type = "string"
description = "Required API key"
"#;
        let schema = ServiceSchema::parse(input, "test").unwrap();
        assert!(schema.params["api_key"].is_required());
    }

    #[test]
    fn invalid_toml_returns_error() {
        let result = ServiceSchema::parse("not valid toml {{{{", "bad");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            crate::CatalogError::InvalidServiceToml { .. }
        ));
    }
}
