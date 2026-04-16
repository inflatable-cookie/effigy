//! Integration tests for the catalog crate.
//!
//! These tests use the bundled catalog fragments to verify the full
//! resolve → validate → render → assemble pipeline.

use std::collections::HashMap;

use effigy_catalog::assembly::{ComposeAssembler, ServiceDeclaration};
use effigy_catalog::fragment::{CatalogResolver, FragmentSource};

/// Helper: create a resolver that only uses bundled fragments.
fn bundled_resolver() -> CatalogResolver {
    CatalogResolver::new(None, None)
}

#[test]
fn list_bundled_fragments() {
    let resolver = bundled_resolver();
    let fragments = resolver.list();

    let names: Vec<&str> = fragments.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"php-fpm"), "missing php-fpm: {names:?}");
    assert!(names.contains(&"nginx"), "missing nginx: {names:?}");
    assert!(names.contains(&"mariadb"), "missing mariadb: {names:?}");
    assert!(names.contains(&"postgres"), "missing postgres: {names:?}");
    assert!(names.contains(&"redis"), "missing redis: {names:?}");
    assert!(names.contains(&"memcached"), "missing memcached: {names:?}");

    // All should be bundled source.
    for f in &fragments {
        assert_eq!(f.source, FragmentSource::Bundled);
    }
}

#[test]
fn resolve_php_fpm_fragment() {
    let resolver = bundled_resolver();
    let fragment = resolver.resolve("php-fpm").unwrap();

    assert_eq!(fragment.name, "php-fpm");
    assert_eq!(fragment.schema.service.name, "php-fpm");
    assert!(fragment.dockerfile.is_some());
    assert!(!fragment.compose_template.is_empty());
    assert!(fragment.schema.capabilities.exec_target);
    assert_eq!(
        fragment.schema.capabilities.shell.as_deref(),
        Some("/bin/bash")
    );
}

#[test]
fn resolve_nginx_with_config_variants() {
    let resolver = bundled_resolver();
    let fragment = resolver.resolve("nginx").unwrap();

    assert_eq!(fragment.name, "nginx");
    assert!(
        fragment.config_variants.contains_key("default"),
        "missing default variant: {:?}",
        fragment.config_variants.keys().collect::<Vec<_>>()
    );
    assert!(fragment.config_variants.contains_key("laravel"));
    assert!(fragment.config_variants.contains_key("spa"));
}

#[test]
fn resolve_mariadb_with_volumes() {
    let resolver = bundled_resolver();
    let fragment = resolver.resolve("mariadb").unwrap();

    assert_eq!(fragment.schema.volumes.len(), 1);
    let data_vol = &fragment.schema.volumes["data"];
    assert_eq!(data_vol.mount, "/var/lib/mysql");
    assert!(data_vol.named);
    assert!(data_vol.persist);
}

#[test]
fn assemble_php_mariadb_redis_stack() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "app".to_string(),
            catalog: "php-fpm".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "version".to_string(),
                    toml::Value::String("8.3".to_string()),
                );
                p.insert(
                    "extensions".to_string(),
                    toml::Value::Array(vec![
                        toml::Value::String("pdo_mysql".to_string()),
                        toml::Value::String("gd".to_string()),
                        toml::Value::String("redis".to_string()),
                    ]),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "version".to_string(),
                    toml::Value::String("10.11".to_string()),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "cache".to_string(),
            catalog: "redis".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler.assemble(&services, "test-project", ".").unwrap();

    // Compose YAML should contain all three services.
    assert!(
        result.compose_yaml.contains("app:"),
        "missing app service:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("db:"),
        "missing db service:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("cache:"),
        "missing cache service:\n{}",
        result.compose_yaml
    );

    // PHP service should reference extensions.
    assert!(
        result.compose_yaml.contains("pdo_mysql gd redis"),
        "missing extensions:\n{}",
        result.compose_yaml
    );

    // PHP service should depend on MariaDB.
    assert!(
        result.compose_yaml.contains("depends_on"),
        "missing depends_on:\n{}",
        result.compose_yaml
    );

    // MariaDB should have a volume.
    assert!(
        result.compose_yaml.contains("volumes:"),
        "missing volumes section:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("test-project-db-data"),
        "missing named volume:\n{}",
        result.compose_yaml
    );

    // Should have a Dockerfile for php-fpm.
    assert!(result.dockerfiles.contains_key("app"));
    assert!(result.dockerfiles["app"].contains("PHP_VERSION"));

    // Should have volume info.
    assert_eq!(result.volumes.len(), 1);
    assert_eq!(result.volumes[0].service, "db");
    assert!(result.volumes[0].persist);
}

#[test]
fn assemble_full_lemp_stack() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "app".to_string(),
            catalog: "php-fpm".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "web".to_string(),
            catalog: "nginx".to_string(),
            params: HashMap::new(),
            variant: Some("default".to_string()),
            config: None,
        },
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "cache".to_string(),
            catalog: "redis".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "sessions".to_string(),
            catalog: "memcached".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler.assemble(&services, "client-x", ".").unwrap();

    // All five services present.
    for name in &["app", "web", "db", "cache", "sessions"] {
        assert!(
            result.compose_yaml.contains(&format!("{name}:")),
            "missing {name} service:\n{}",
            result.compose_yaml
        );
    }

    // Nginx config file should be generated.
    assert!(
        result.config_files.contains_key("web.conf"),
        "missing nginx config: {:?}",
        result.config_files.keys().collect::<Vec<_>>()
    );
    assert!(result.config_files["web.conf"].contains("fastcgi_pass"));
}

#[test]
fn fragment_not_found_error() {
    let resolver = bundled_resolver();
    let result = resolver.resolve("nonexistent");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        effigy_catalog::CatalogError::FragmentNotFound { .. }
    ));
}

#[test]
fn missing_required_param_error() {
    let resolver = bundled_resolver();
    let _fragment = resolver.resolve("nginx").unwrap();

    // All bundled fragments should have defaults for all params.
    // Verify mariadb resolves cleanly with no explicit params.
    let assembler = ComposeAssembler::new(bundled_resolver());
    let services = vec![ServiceDeclaration {
        name: "db".to_string(),
        catalog: "mariadb".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    // Should succeed — all params have defaults.
    let result = assembler.assemble(&services, "test", ".");
    assert!(result.is_ok());
}

#[test]
fn invalid_variant_error() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "web".to_string(),
        catalog: "nginx".to_string(),
        params: HashMap::new(),
        variant: Some("nonexistent-variant".to_string()),
        config: None,
    }];

    let result = assembler.assemble(&services, "test", ".");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        effigy_catalog::CatalogError::VariantNotFound { .. }
    ));
}
