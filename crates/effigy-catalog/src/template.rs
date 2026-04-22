//! Template rendering via minijinja (Jinja2 syntax).
//!
//! Renders compose fragment templates with a context containing:
//! - fragment parameters (from manifest service declaration)
//! - sibling service info (for depends_on references)
//! - system variables (repo_root, catalog_path, project_name, host_uid, host_gid)

use std::collections::HashMap;

use minijinja::{Environment, Value};
use serde::Serialize;

use crate::error::CatalogError;
use crate::schema::{ParamSchema, ParamType, ServiceSchema};

/// Renders Jinja2 templates with validated parameters.
pub struct TemplateRenderer {
    env: Environment<'static>,
}

impl TemplateRenderer {
    /// Create a new renderer.
    pub fn new() -> Self {
        let env = Environment::new();
        Self { env }
    }

    /// Render a compose fragment template with the given context.
    pub fn render(
        &self,
        template_source: &str,
        context: &TemplateContext,
        fragment_name: &str,
    ) -> Result<String, CatalogError> {
        let mut env = self.env.clone();
        env.add_template("fragment", template_source).map_err(|e| {
            CatalogError::TemplateRenderError {
                name: fragment_name.to_string(),
                reason: format!("template parse error: {e}"),
            }
        })?;

        let tmpl = env
            .get_template("fragment")
            .map_err(|e| CatalogError::TemplateRenderError {
                name: fragment_name.to_string(),
                reason: e.to_string(),
            })?;

        tmpl.render(context)
            .map_err(|e| CatalogError::TemplateRenderError {
                name: fragment_name.to_string(),
                reason: format!("render error: {e}"),
            })
    }

    /// Validate parameters against the schema and build a template context.
    pub fn build_context(
        schema: &ServiceSchema,
        service_name: &str,
        params: &HashMap<String, toml::Value>,
        siblings: &HashMap<String, SiblingService>,
        system: &SystemContext,
    ) -> Result<TemplateContext, CatalogError> {
        let mut validated = HashMap::new();

        // Validate and resolve each declared parameter.
        for (name, decl) in &schema.params {
            let value = if let Some(provided) = params.get(name) {
                Self::validate_param(service_name, name, provided, decl)?;
                toml_to_minijinja(provided)
            } else if let Some(ref default) = decl.default {
                toml_to_minijinja(default)
            } else {
                return Err(CatalogError::MissingRequiredParam {
                    service: service_name.to_string(),
                    param: name.to_string(),
                });
            };
            validated.insert(name.clone(), value);
        }

        // Reject parameter names that collide with system context fields.
        // These are reserved names in the template context.
        const RESERVED: &[&str] = &[
            "service_name",
            "services",
            "repo_root",
            "catalog_path",
            "project_name",
            "host_uid",
            "host_gid",
        ];
        for name in validated.keys() {
            if RESERVED.contains(&name.as_str()) {
                return Err(CatalogError::ParamTypeMismatch {
                    service: service_name.to_string(),
                    param: name.to_string(),
                    expected: "non-reserved name".to_string(),
                    actual: format!("'{name}' is reserved (collides with system context field)"),
                });
            }
        }

        Ok(TemplateContext {
            params: validated,
            service_name: service_name.to_string(),
            services: siblings.clone(),
            repo_root: system.repo_root.clone(),
            catalog_path: system.catalog_path.clone(),
            project_name: system.project_name.clone(),
            host_uid: system.host_uid,
            host_gid: system.host_gid,
        })
    }

    /// Validate a single parameter value against its schema.
    fn validate_param(
        service_name: &str,
        param_name: &str,
        value: &toml::Value,
        schema: &ParamSchema,
    ) -> Result<(), CatalogError> {
        let type_ok = match schema.param_type {
            ParamType::String => value.is_str(),
            ParamType::List => value.is_array(),
            ParamType::Bool => matches!(value, toml::Value::Boolean(_)),
            ParamType::Integer => value.is_integer(),
        };

        if !type_ok {
            return Err(CatalogError::ParamTypeMismatch {
                service: service_name.to_string(),
                param: param_name.to_string(),
                expected: schema.param_type.to_string(),
                actual: toml_type_name(value).to_string(),
            });
        }

        Ok(())
    }
}

impl Default for TemplateRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Context passed to a fragment template during rendering.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateContext {
    /// Validated parameter values.
    #[serde(flatten)]
    pub params: HashMap<String, Value>,

    /// The name assigned to this service in the compose file.
    pub service_name: String,

    /// Other services in the stack, keyed by their role name.
    pub services: HashMap<String, SiblingService>,

    /// Repo root path (for bind mounts).
    pub repo_root: String,

    /// Resolved path to the fragment's catalog directory.
    pub catalog_path: String,

    /// Compose project name.
    pub project_name: String,

    /// Host user ID for bind-mounted workspace ownership alignment.
    pub host_uid: u32,

    /// Host group ID for bind-mounted workspace ownership alignment.
    pub host_gid: u32,
}

/// Information about a sibling service in the stack.
#[derive(Debug, Clone, Serialize)]
pub struct SiblingService {
    /// Service name in the compose file.
    pub name: String,

    /// Which catalog fragment this sibling uses.
    pub catalog: String,

    /// Expose port (if known).
    pub port: Option<u16>,
}

/// System-level context variables.
#[derive(Debug, Clone)]
pub struct SystemContext {
    /// Repo root path.
    pub repo_root: String,

    /// Resolved catalog path for the fragment.
    pub catalog_path: String,

    /// Compose project name.
    pub project_name: String,

    /// Host user ID for bind-mounted workspace ownership alignment.
    pub host_uid: u32,

    /// Host group ID for bind-mounted workspace ownership alignment.
    pub host_gid: u32,
}

/// Convert a TOML value to a minijinja Value.
fn toml_to_minijinja(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::from(s.as_str()),
        toml::Value::Integer(i) => Value::from(*i),
        toml::Value::Float(f) => Value::from(*f),
        toml::Value::Boolean(b) => Value::from(*b),
        toml::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(toml_to_minijinja).collect();
            Value::from(items)
        }
        toml::Value::Table(tbl) => {
            let map: HashMap<String, Value> = tbl
                .iter()
                .map(|(k, v)| (k.clone(), toml_to_minijinja(v)))
                .collect();
            Value::from_serialize(&map)
        }
        toml::Value::Datetime(dt) => Value::from(dt.to_string()),
    }
}

/// Human-readable type name for a TOML value.
fn toml_type_name(v: &toml::Value) -> &'static str {
    match v {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "bool",
        toml::Value::Array(_) => "list",
        toml::Value::Table(_) => "table",
        toml::Value::Datetime(_) => "datetime",
    }
}

#[cfg(test)]
#[path = "template/tests.rs"]
mod tests;
