use super::*;
use crate::fragment::{CatalogFragment, FragmentSource};
use crate::schema::ServiceSchema;

#[test]
fn sibling_map_builds_correctly() {
    let schema_str = r#"
[service]
name = "mariadb"

[ports]
default = [3306]
"#;
    let schema = ServiceSchema::parse(schema_str, "mariadb").unwrap();

    let mut fragments = IndexMap::new();
    let decl = ServiceDeclaration {
        name: "db".to_string(),
        catalog: "mariadb".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    };
    let fragment = CatalogFragment {
        name: "mariadb".to_string(),
        schema,
        compose_template: String::new(),
        dockerfile: None,
        config_variants: HashMap::new(),
        param_variants: HashMap::new(),
        source: FragmentSource::Bundled,
    };
    fragments.insert("db".to_string(), (decl, fragment));

    let resolved_params = HashMap::from([("db".to_string(), HashMap::new())]);
    let siblings = ComposeAssembler::build_sibling_map(&fragments, &resolved_params);

    // Accessible by catalog name.
    assert!(siblings.contains_key("mariadb"));
    assert_eq!(siblings["mariadb"].name, "db");
    assert_eq!(siblings["mariadb"].port, Some(3306));

    // Accessible by service name.
    assert!(siblings.contains_key("db"));
    assert_eq!(siblings["db"].catalog, "mariadb");
}

#[test]
fn extract_service_definition_from_valid_yaml() {
    let yaml = "services:\n  app:\n    image: php:8.3\n    ports:\n      - '9000:9000'\n";
    let def = ComposeAssembler::extract_service_definition(yaml, "app", "test").unwrap();
    assert!(def.get("image").is_some());
    assert_eq!(def.get("image").unwrap().as_str().unwrap(), "php:8.3");
}

#[test]
fn extract_service_definition_rejects_invalid_yaml() {
    let yaml = "not: valid: yaml: {{{{";
    let result = ComposeAssembler::extract_service_definition(yaml, "app", "test");
    assert!(result.is_err());
}

#[test]
fn extract_service_definition_rejects_missing_services_key() {
    let yaml = "app:\n  image: php:8.3\n";
    let result = ComposeAssembler::extract_service_definition(yaml, "app", "test");
    assert!(result.is_err());
}

#[test]
fn build_compose_document_with_volumes() {
    let mut services = serde_yaml::Mapping::new();
    let mut app = serde_yaml::Mapping::new();
    app.insert(
        YamlValue::String("image".to_string()),
        YamlValue::String("test:latest".to_string()),
    );
    services.insert(
        YamlValue::String("app".to_string()),
        YamlValue::Mapping(app),
    );

    let volumes = vec![VolumeInfo {
        name: "test-data".to_string(),
        named: true,
        persist: true,
        service: "db".to_string(),
    }];

    let yaml = ComposeAssembler::build_compose_document(services, &volumes).unwrap();
    assert!(yaml.contains("services:"));
    assert!(yaml.contains("app:"));
    assert!(yaml.contains("volumes:"));
    assert!(yaml.contains("test-data:"));
    assert!(yaml.contains("driver: local"));
}

#[test]
fn build_compose_document_without_volumes() {
    let mut services = serde_yaml::Mapping::new();
    let mut app = serde_yaml::Mapping::new();
    app.insert(
        YamlValue::String("image".to_string()),
        YamlValue::String("test:latest".to_string()),
    );
    services.insert(
        YamlValue::String("app".to_string()),
        YamlValue::Mapping(app),
    );

    let yaml = ComposeAssembler::build_compose_document(services, &[]).unwrap();
    assert!(yaml.contains("services:"));
    assert!(!yaml.contains("volumes:"));
}

#[test]
fn empty_service_list_rejected() {
    let resolver = CatalogResolver::new(None, None);
    let assembler = ComposeAssembler::new(resolver);
    let result = assembler.assemble(&[], "test", ".", ".effigy-catalog", 1000, 1000);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CatalogError::EmptyServiceList
    ));
}

#[test]
fn fragment_build_context_path_uses_supplied_catalog_root() {
    assert_eq!(
        ComposeAssembler::fragment_build_context_path(
            ".effigy/runtime/compose/.effigy-catalog",
            "app"
        ),
        ".effigy/runtime/compose/.effigy-catalog/app"
    );
    assert_eq!(
        ComposeAssembler::fragment_build_context_path(".effigy-catalog/", "app"),
        ".effigy-catalog/app"
    );
}
