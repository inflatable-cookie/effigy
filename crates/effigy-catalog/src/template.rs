//! Template rendering via minijinja (Jinja2 syntax).
//!
//! Renders compose fragment templates with a context containing:
//! - fragment parameters (from manifest service declaration)
//! - sibling service info (for depends_on references)
//! - system variables (repo_root, catalog_path, project_name)

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

        // Warn about (but allow) unknown params — they might be for overrides.
        // In the future we can make this stricter.

        Ok(TemplateContext {
            params: validated,
            service_name: service_name.to_string(),
            services: siblings.clone(),
            repo_root: system.repo_root.clone(),
            catalog_path: system.catalog_path.clone(),
            project_name: system.project_name.clone(),
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
mod tests {
    use super::*;
    use crate::schema::ServiceSchema;

    fn test_schema() -> ServiceSchema {
        let toml_str = r#"
[service]
name = "test"

[params.version]
type = "string"
default = "1.0"
description = "Version"

[params.extensions]
type = "list"
default = []
description = "Extensions"

[params.debug]
type = "bool"
default = false
description = "Debug mode"
"#;
        ServiceSchema::parse(toml_str, "test").unwrap()
    }

    #[test]
    fn build_context_with_defaults() {
        let schema = test_schema();
        let params = HashMap::new();
        let siblings = HashMap::new();
        let system = SystemContext {
            repo_root: ".".to_string(),
            catalog_path: "/catalog/test".to_string(),
            project_name: "test-project".to_string(),
        };

        let ctx =
            TemplateRenderer::build_context(&schema, "app", &params, &siblings, &system).unwrap();

        assert_eq!(ctx.service_name, "app");
        assert_eq!(ctx.project_name, "test-project");
    }

    #[test]
    fn build_context_with_overrides() {
        let schema = test_schema();
        let mut params = HashMap::new();
        params.insert(
            "version".to_string(),
            toml::Value::String("2.0".to_string()),
        );
        params.insert(
            "extensions".to_string(),
            toml::Value::Array(vec![
                toml::Value::String("gd".to_string()),
                toml::Value::String("redis".to_string()),
            ]),
        );

        let siblings = HashMap::new();
        let system = SystemContext {
            repo_root: ".".to_string(),
            catalog_path: "/catalog/test".to_string(),
            project_name: "proj".to_string(),
        };

        let ctx =
            TemplateRenderer::build_context(&schema, "web", &params, &siblings, &system).unwrap();

        assert_eq!(ctx.service_name, "web");
    }

    #[test]
    fn type_mismatch_rejected() {
        let schema = test_schema();
        let mut params = HashMap::new();
        params.insert("version".to_string(), toml::Value::Integer(42));

        let siblings = HashMap::new();
        let system = SystemContext {
            repo_root: ".".to_string(),
            catalog_path: "".to_string(),
            project_name: "".to_string(),
        };

        let result = TemplateRenderer::build_context(&schema, "app", &params, &siblings, &system);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CatalogError::ParamTypeMismatch { .. }
        ));
    }

    #[test]
    fn render_simple_template() {
        let renderer = TemplateRenderer::new();
        let ctx = TemplateContext {
            params: {
                let mut m = HashMap::new();
                m.insert("version".to_string(), Value::from("8.3"));
                m
            },
            service_name: "app".to_string(),
            services: HashMap::new(),
            repo_root: ".".to_string(),
            catalog_path: "/catalog/php-fpm".to_string(),
            project_name: "test".to_string(),
        };

        let template = r#"services:
  {{ service_name }}:
    image: php:{{ version }}-fpm"#;

        let result = renderer.render(template, &ctx, "test").unwrap();
        assert!(result.contains("app:"));
        assert!(result.contains("php:8.3-fpm"));
    }

    #[test]
    fn render_conditional_depends_on() {
        let renderer = TemplateRenderer::new();
        let mut siblings = HashMap::new();
        siblings.insert(
            "db".to_string(),
            SiblingService {
                name: "database".to_string(),
                catalog: "mariadb".to_string(),
                port: Some(3306),
            },
        );

        let ctx = TemplateContext {
            params: HashMap::new(),
            service_name: "app".to_string(),
            services: siblings,
            repo_root: ".".to_string(),
            catalog_path: "".to_string(),
            project_name: "test".to_string(),
        };

        let template = r#"services:
  {{ service_name }}:
    image: php:8.3-fpm
{% if services.db %}
    depends_on:
      {{ services.db.name }}:
        condition: service_started
{% endif %}"#;

        let result = renderer.render(template, &ctx, "test").unwrap();
        assert!(result.contains("depends_on:"));
        assert!(result.contains("database:"));
    }

    #[test]
    fn render_list_join() {
        let renderer = TemplateRenderer::new();
        let ctx = TemplateContext {
            params: {
                let mut m = HashMap::new();
                m.insert(
                    "extensions".to_string(),
                    Value::from(vec!["gd", "redis", "pdo_mysql"]),
                );
                m
            },
            service_name: "app".to_string(),
            services: HashMap::new(),
            repo_root: ".".to_string(),
            catalog_path: "".to_string(),
            project_name: "test".to_string(),
        };

        let template = r#"args:
  EXTENSIONS: "{{ extensions | join(' ') }}""#;

        let result = renderer.render(template, &ctx, "test").unwrap();
        assert!(result.contains("gd redis pdo_mysql"));
    }
}
