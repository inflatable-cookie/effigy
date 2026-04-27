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
        host_uid: 501,
        host_gid: 20,
    };

    let ctx = TemplateRenderer::build_context(&schema, "app", &params, &siblings, &system).unwrap();

    assert_eq!(ctx.service_name, "app");
    assert_eq!(ctx.project_name, "test-project");
    assert_eq!(ctx.host_uid, 501);
    assert_eq!(ctx.host_gid, 20);
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
        host_uid: 501,
        host_gid: 20,
    };

    let ctx = TemplateRenderer::build_context(&schema, "web", &params, &siblings, &system).unwrap();

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
        host_uid: 501,
        host_gid: 20,
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
        host_uid: 501,
        host_gid: 20,
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
            params: HashMap::new(),
        },
    );

    let ctx = TemplateContext {
        params: HashMap::new(),
        service_name: "app".to_string(),
        services: siblings,
        repo_root: ".".to_string(),
        catalog_path: "".to_string(),
        project_name: "test".to_string(),
        host_uid: 501,
        host_gid: 20,
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
fn render_sibling_params_lookup() {
    let renderer = TemplateRenderer::new();
    let mut siblings = HashMap::new();
    siblings.insert(
        "db".to_string(),
        SiblingService {
            name: "database".to_string(),
            catalog: "mariadb".to_string(),
            port: Some(3306),
            params: {
                let mut params = HashMap::new();
                params.insert("password".to_string(), Value::from("localdev"));
                params
            },
        },
    );

    let ctx = TemplateContext {
        params: {
            let mut m = HashMap::new();
            m.insert("database_host".to_string(), Value::from("db"));
            m
        },
        service_name: "dbadmin".to_string(),
        services: siblings,
        repo_root: ".".to_string(),
        catalog_path: "".to_string(),
        project_name: "test".to_string(),
        host_uid: 501,
        host_gid: 20,
    };

    let template = r#"{% set database_service = services[database_host] %}{{ database_service.params.password }}"#;

    let result = renderer.render(template, &ctx, "test").unwrap();
    assert_eq!(result, "localdev");
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
        host_uid: 501,
        host_gid: 20,
    };

    let template = r#"args:
  EXTENSIONS: "{{ extensions | join(' ') }}""#;

    let result = renderer.render(template, &ctx, "test").unwrap();
    assert!(result.contains("gd redis pdo_mysql"));
}
