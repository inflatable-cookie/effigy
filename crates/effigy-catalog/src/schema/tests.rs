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
