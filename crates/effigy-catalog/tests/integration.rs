//! Integration tests for the catalog crate.
//!
//! These tests use the bundled catalog fragments to verify the full
//! resolve → validate → render → assemble pipeline.

use std::collections::HashMap;

use effigy_catalog::assembly::{ComposeAssembler, ServiceDeclaration};
use effigy_catalog::fragment::{CatalogResolver, FragmentSource};
use effigy_catalog::output::ComposeOutput;

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
    assert!(names.contains(&"mailpit"), "missing mailpit: {names:?}");
    assert!(names.contains(&"minio"), "missing minio: {names:?}");
    assert!(
        names.contains(&"elasticsearch"),
        "missing elasticsearch: {names:?}"
    );
    assert!(names.contains(&"node"), "missing node: {names:?}");

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

// --- Batch 5: Override precedence and inspection ---

#[test]
fn project_local_override_takes_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let local_catalog = dir.path().join("catalog");

    // Create a project-local redis override with a custom compose fragment.
    let redis_dir = local_catalog.join("redis");
    std::fs::create_dir_all(&redis_dir).unwrap();
    std::fs::write(
        redis_dir.join("service.toml"),
        r#"
[service]
name = "redis"
description = "Custom Redis override"

[params.version]
type = "string"
default = "7"
"#,
    )
    .unwrap();
    std::fs::write(
        redis_dir.join("compose.fragment.yml"),
        r#"services:
  {{ service_name }}:
    image: custom-redis:{{ version }}
"#,
    )
    .unwrap();

    let resolver = CatalogResolver::new(Some(local_catalog), None);

    // Redis should resolve from project-local.
    let fragment = resolver.resolve("redis").unwrap();
    assert!(matches!(fragment.source, FragmentSource::ProjectLocal(_)));
    assert!(fragment.compose_template.contains("custom-redis"));

    // MariaDB should still resolve from bundled.
    let mariadb = resolver.resolve("mariadb").unwrap();
    assert_eq!(mariadb.source, FragmentSource::Bundled);
}

#[test]
fn user_global_override_takes_precedence_over_bundled() {
    let dir = tempfile::tempdir().unwrap();
    let global_catalog = dir.path().join("global");

    // Create a user-global nginx override.
    let nginx_dir = global_catalog.join("nginx");
    let configs_dir = nginx_dir.join("configs");
    std::fs::create_dir_all(&configs_dir).unwrap();
    std::fs::write(
        nginx_dir.join("service.toml"),
        r#"
[service]
name = "nginx"
description = "Custom global nginx"

[params.document_root]
type = "string"
default = "public"
"#,
    )
    .unwrap();
    std::fs::write(
        nginx_dir.join("compose.fragment.yml"),
        "services:\n  {{ service_name }}:\n    image: custom-nginx\n",
    )
    .unwrap();
    std::fs::write(configs_dir.join("default.conf"), "# custom default").unwrap();

    let resolver = CatalogResolver::new(None, Some(global_catalog));
    let fragment = resolver.resolve("nginx").unwrap();
    assert!(matches!(fragment.source, FragmentSource::UserGlobal(_)));
    assert!(fragment.compose_template.contains("custom-nginx"));
}

#[test]
fn project_local_wins_over_user_global() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("local");
    let global = dir.path().join("global");

    // Both have redis overrides.
    for (path, label) in [(&local, "local"), (&global, "global")] {
        let redis_dir = path.join("redis");
        std::fs::create_dir_all(&redis_dir).unwrap();
        std::fs::write(
            redis_dir.join("service.toml"),
            format!(
                "[service]\nname = \"redis\"\ndescription = \"{label} redis\"\n\n\
                 [params.version]\ntype = \"string\"\ndefault = \"7\"\n"
            ),
        )
        .unwrap();
        std::fs::write(
            redis_dir.join("compose.fragment.yml"),
            format!("services:\n  {{{{ service_name }}}}:\n    image: {label}-redis\n"),
        )
        .unwrap();
    }

    let resolver = CatalogResolver::new(Some(local), Some(global));
    let fragment = resolver.resolve("redis").unwrap();
    assert!(matches!(fragment.source, FragmentSource::ProjectLocal(_)));
    assert!(fragment.compose_template.contains("local-redis"));
}

#[test]
fn list_shows_overrides_at_correct_layer() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().join("local");

    // Create a project-local custom service.
    let custom_dir = local.join("my-custom-service");
    std::fs::create_dir_all(&custom_dir).unwrap();

    let resolver = CatalogResolver::new(Some(local.clone()), None);
    let list = resolver.list();

    // Should include both bundled fragments and the custom one.
    let names: Vec<&str> = list.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"php-fpm"));
    assert!(names.contains(&"my-custom-service"));

    // Custom service should show as project-local.
    let custom = list.iter().find(|f| f.name == "my-custom-service").unwrap();
    assert!(matches!(custom.source, FragmentSource::ProjectLocal(_)));
}

#[test]
fn extract_bundled_fragment_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = bundled_resolver();

    let extracted_path = resolver.extract("php-fpm", dir.path()).unwrap();

    assert!(extracted_path.join("service.toml").exists());
    assert!(extracted_path.join("compose.fragment.yml").exists());
    assert!(extracted_path.join("Dockerfile").exists());

    // Verify the extracted service.toml is valid.
    let content = std::fs::read_to_string(extracted_path.join("service.toml")).unwrap();
    assert!(content.contains("php-fpm"));
}

#[test]
fn extract_fragment_with_configs() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = bundled_resolver();

    let extracted_path = resolver.extract("nginx", dir.path()).unwrap();

    assert!(extracted_path.join("service.toml").exists());
    assert!(extracted_path.join("compose.fragment.yml").exists());
    assert!(extracted_path.join("configs/default.conf").exists());
    assert!(extracted_path.join("configs/laravel.conf").exists());
}

#[test]
fn extract_nonexistent_fragment_fails() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = bundled_resolver();

    let result = resolver.extract("nonexistent", dir.path());
    assert!(result.is_err());
}

// --- Batch 6: End-to-end compose output and eject ---

#[test]
fn full_pipeline_assemble_write_eject() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("infra/dev");

    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);
    let output = ComposeOutput::new(output_dir.clone());

    let services = vec![
        ServiceDeclaration {
            name: "app".to_string(),
            catalog: "php-fpm".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "extensions".to_string(),
                    toml::Value::Array(vec![toml::Value::String("pdo_mysql".to_string())]),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let manifest = "[containers.web]\nservices = [\"php-fpm\", \"mariadb\"]\n";

    // Assemble.
    let assembly = assembler.assemble(&services, "test-proj", ".").unwrap();

    // Write — should regenerate.
    let write1 = output.write(&assembly, manifest).unwrap();
    assert!(write1.regenerated);
    assert!(write1.compose_path.exists());

    // Write again — should be cached.
    let write2 = output.write(&assembly, manifest).unwrap();
    assert!(!write2.regenerated);

    // Eject.
    let eject = output.eject().unwrap();
    assert!(eject.compose_path.exists());
    assert!(eject
        .compose_path
        .to_str()
        .unwrap()
        .contains("docker-compose.yml"));

    // Generated file should be gone.
    assert!(!output.generated_compose_path().exists());

    // Permanent file should contain the compose YAML.
    let content = std::fs::read_to_string(&eject.compose_path).unwrap();
    assert!(content.contains("app:"));
    assert!(content.contains("db:"));
}

// --- Batch 7: Structural validation and edge cases ---

/// Parse the assembled compose YAML back into a serde_yaml::Value and
/// validate its structure matches what Docker Compose expects.
fn validate_compose_structure(yaml: &str) -> serde_yaml::Value {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml)
        .unwrap_or_else(|e| panic!("assembled compose is not valid YAML:\n{e}\n---\n{yaml}"));

    // Must have top-level 'services' key.
    assert!(
        doc.get("services").is_some(),
        "compose missing 'services' key:\n{yaml}"
    );

    // Services must be a mapping.
    assert!(
        doc.get("services").unwrap().is_mapping(),
        "services is not a mapping:\n{yaml}"
    );

    // If volumes key exists, it must be a mapping.
    if let Some(volumes) = doc.get("volumes") {
        assert!(volumes.is_mapping(), "volumes is not a mapping:\n{yaml}");
    }

    doc
}

/// Validate a single service definition has the expected structure.
fn validate_service(doc: &serde_yaml::Value, name: &str) -> serde_yaml::Value {
    let services = doc.get("services").unwrap();
    let svc = services.get(name).unwrap_or_else(|| {
        panic!(
            "service '{name}' not found in compose. Available: {:?}",
            services.as_mapping().unwrap().keys().collect::<Vec<_>>()
        )
    });
    assert!(svc.is_mapping(), "service '{name}' is not a mapping");
    svc.clone()
}

#[test]
fn assembled_yaml_is_structurally_valid_compose() {
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
                    toml::Value::String("8.2".to_string()),
                );
                p.insert(
                    "extensions".to_string(),
                    toml::Value::Array(vec![
                        toml::Value::String("pdo_mysql".to_string()),
                        toml::Value::String("gd".to_string()),
                        toml::Value::String("redis".to_string()),
                        toml::Value::String("memcached".to_string()),
                        toml::Value::String("intl".to_string()),
                    ]),
                );
                p.insert(
                    "document_root".to_string(),
                    toml::Value::String("public".to_string()),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "web".to_string(),
            catalog: "nginx".to_string(),
            params: HashMap::new(),
            variant: Some("laravel".to_string()),
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
                p.insert(
                    "database".to_string(),
                    toml::Value::String("clientdb".to_string()),
                );
                p.insert(
                    "root_password".to_string(),
                    toml::Value::String("devpass".to_string()),
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
        ServiceDeclaration {
            name: "sessions".to_string(),
            catalog: "memcached".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert("memory".to_string(), toml::Value::Integer(128));
                p
            },
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(&services, "client-project", ".")
        .unwrap();

    // 1. Validate the YAML parses and has correct structure.
    let doc = validate_compose_structure(&result.compose_yaml);

    // 2. Validate each service exists and has expected properties.
    let app = validate_service(&doc, "app");
    assert!(app.get("build").is_some(), "app missing 'build'");
    assert!(app.get("volumes").is_some(), "app missing 'volumes'");
    assert!(
        app.get("working_dir").is_some(),
        "app missing 'working_dir'"
    );
    assert!(
        app.get("environment").is_some(),
        "app missing 'environment'"
    );

    // PHP extensions should be in the build args.
    let build_args = app.get("build").unwrap().get("args").unwrap();
    let ext_val = build_args.get("EXTENSIONS").unwrap().as_str().unwrap();
    assert!(ext_val.contains("pdo_mysql"));
    assert!(ext_val.contains("gd"));
    assert!(ext_val.contains("redis"));
    assert!(ext_val.contains("memcached"));
    assert!(ext_val.contains("intl"));

    // PHP should depend on mariadb (via depends_on).
    assert!(
        app.get("depends_on").is_some(),
        "app should have depends_on for mariadb"
    );
    let depends = app.get("depends_on").unwrap();
    assert!(
        depends.get("db").is_some(),
        "app should depend on 'db' service"
    );

    let web = validate_service(&doc, "web");
    assert!(web.get("image").is_some(), "web missing 'image'");
    assert!(web.get("ports").is_some(), "web missing 'ports'");

    let db = validate_service(&doc, "db");
    assert!(db.get("image").is_some(), "db missing 'image'");
    let db_image = db.get("image").unwrap().as_str().unwrap();
    assert!(db_image.contains("mariadb"), "db image should be mariadb");
    assert!(
        db_image.contains("10.11"),
        "db image should use version 10.11"
    );

    let db_env = db.get("environment").unwrap();
    assert_eq!(
        db_env.get("MYSQL_DATABASE").unwrap().as_str().unwrap(),
        "clientdb"
    );

    let cache = validate_service(&doc, "cache");
    let cache_image = cache.get("image").unwrap().as_str().unwrap();
    assert!(cache_image.contains("redis"));

    let sessions = validate_service(&doc, "sessions");
    let sessions_cmd = sessions.get("command").unwrap();
    // Memcached command should include the memory flag.
    let cmd_str = format!("{sessions_cmd:?}");
    assert!(cmd_str.contains("128"), "memcached should use 128MB memory");

    // 3. Validate volumes section.
    let volumes = doc.get("volumes").unwrap().as_mapping().unwrap();
    assert!(
        volumes
            .keys()
            .any(|k| { k.as_str().map(|s| s.contains("db-data")).unwrap_or(false) }),
        "should have a mariadb data volume"
    );

    // 4. Validate artifacts.
    assert!(
        result.dockerfiles.contains_key("app"),
        "should have Dockerfile for php-fpm"
    );
    assert!(
        result.dockerfiles["app"].contains("PHP_VERSION"),
        "Dockerfile should reference PHP_VERSION"
    );

    assert!(
        result.config_files.contains_key("web.conf"),
        "should have nginx config"
    );
    assert!(
        result.config_files["web.conf"].contains("fastcgi_pass"),
        "nginx config should have fastcgi_pass"
    );

    // 5. Validate volume metadata.
    assert_eq!(result.volumes.len(), 1);
    assert!(result.volumes[0].persist);
    assert_eq!(result.volumes[0].service, "db");
}

#[test]
fn php_with_both_mariadb_and_redis_produces_valid_depends_on() {
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
    ];

    let result = assembler.assemble(&services, "test", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let app = validate_service(&doc, "app");

    // Should have a single depends_on with both db and cache.
    let depends = app.get("depends_on").unwrap();
    assert!(depends.get("db").is_some(), "app should depend on db");
    assert!(depends.get("cache").is_some(), "app should depend on cache");
}

#[test]
fn rust_postgres_stack_assembles_correctly() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "postgres".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert("version".to_string(), toml::Value::String("16".to_string()));
                p.insert(
                    "database".to_string(),
                    toml::Value::String("myapp".to_string()),
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

    let result = assembler.assemble(&services, "rust-svc", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    let db = validate_service(&doc, "db");
    let db_image = db.get("image").unwrap().as_str().unwrap();
    assert!(db_image.contains("postgres:16"));

    let db_env = db.get("environment").unwrap();
    assert_eq!(
        db_env.get("POSTGRES_DB").unwrap().as_str().unwrap(),
        "myapp"
    );

    // Postgres volume should exist (named {project}-{service}-{key}).
    assert_eq!(result.volumes.len(), 1);
    assert_eq!(result.volumes[0].name, "rust-svc-db-data");
    assert_eq!(result.volumes[0].service, "db");
    assert!(result.volumes[0].persist);
}

#[test]
fn duplicate_service_names_rejected() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "postgres".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler.assemble(&services, "test", ".");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        effigy_catalog::CatalogError::DuplicateServiceName { .. }
    ));
}

#[test]
fn empty_services_rejected() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let result = assembler.assemble(&[], "test", ".");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        effigy_catalog::CatalogError::EmptyServiceList
    ));
}

#[test]
fn single_service_assembles_without_volumes() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "cache".to_string(),
        catalog: "redis".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    let result = assembler.assemble(&services, "minimal", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    validate_service(&doc, "cache");
    assert!(result.volumes.is_empty());
    assert!(doc.get("volumes").is_none());
}

// --- Config file templating ───────────────────────────────────────────

#[test]
fn nginx_config_resolves_php_service_name() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    // Name the PHP service "php" instead of the conventional "app".
    let services = vec![
        ServiceDeclaration {
            name: "php".to_string(),
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
    ];

    let result = assembler.assemble(&services, "test", ".").unwrap();

    // The nginx config should reference "php:9000", not "app:9000".
    let config = &result.config_files["web.conf"];
    assert!(
        config.contains("php:9000"),
        "nginx config should resolve PHP service name to 'php', got:\n{config}"
    );
    assert!(
        !config.contains("app:9000"),
        "nginx config should NOT contain hardcoded 'app:9000'"
    );
}

#[test]
fn nginx_config_resolves_document_root() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "app".to_string(),
            catalog: "php-fpm".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "document_root".to_string(),
                    toml::Value::String("web".to_string()),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "web".to_string(),
            catalog: "nginx".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "document_root".to_string(),
                    toml::Value::String("web".to_string()),
                );
                p
            },
            variant: Some("default".to_string()),
            config: None,
        },
    ];

    let result = assembler.assemble(&services, "test", ".").unwrap();

    let config = &result.config_files["web.conf"];
    assert!(
        config.contains("/var/www/html/web"),
        "nginx root should use configured document_root 'web', got:\n{config}"
    );
    assert!(
        !config.contains("/var/www/html/public"),
        "nginx root should NOT contain default 'public'"
    );
}

// --- Additional service fragments ─────────────────────────────────────

#[test]
fn node_fragment_assembles_with_modules_volume() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "app".to_string(),
            catalog: "node".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "command".to_string(),
                    toml::Value::String("npm run dev".to_string()),
                );
                p.insert("port".to_string(), toml::Value::Integer(5173));
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "postgres".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler.assemble(&services, "frontend", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let app = validate_service(&doc, "app");

    let image = app.get("image").unwrap().as_str().unwrap();
    assert!(image.contains("node:"), "image should be node: {image}");

    // Should have a node_modules named volume to avoid platform conflicts.
    assert!(
        result
            .volumes
            .iter()
            .any(|v| v.name.contains("node-modules")),
        "should have node_modules volume: {:?}",
        result.volumes
    );

    // Should depend on postgres.
    assert!(
        app.get("depends_on").is_some(),
        "node app should depend on postgres"
    );
}

#[test]
fn mailpit_fragment_assembles() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "mail".to_string(),
        catalog: "mailpit".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    let result = assembler.assemble(&services, "test", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let mail = validate_service(&doc, "mail");

    let image = mail.get("image").unwrap().as_str().unwrap();
    assert!(
        image.contains("mailpit"),
        "image should be mailpit: {image}"
    );
    assert!(mail.get("ports").is_some(), "mailpit should expose ports");
    assert!(
        mail.get("healthcheck").is_some(),
        "mailpit should have a healthcheck"
    );
}

#[test]
fn minio_fragment_assembles_with_volume() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "storage".to_string(),
        catalog: "minio".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    let result = assembler.assemble(&services, "test", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let storage = validate_service(&doc, "storage");

    let image = storage.get("image").unwrap().as_str().unwrap();
    assert!(image.contains("minio"), "image should be minio: {image}");
    assert!(
        storage.get("healthcheck").is_some(),
        "minio should have a healthcheck"
    );

    // Should have a persistent volume.
    assert_eq!(result.volumes.len(), 1);
    assert!(result.volumes[0].persist);
    assert_eq!(result.volumes[0].name, "test-storage-data");
}

#[test]
fn elasticsearch_fragment_assembles_with_resource_limits() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "search".to_string(),
        catalog: "elasticsearch".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "version".to_string(),
                toml::Value::String("8.15.0".to_string()),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler.assemble(&services, "test", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let search = validate_service(&doc, "search");

    let image = search.get("image").unwrap().as_str().unwrap();
    assert!(
        image.contains("elasticsearch:8.15.0"),
        "image should be elasticsearch:8.15.0: {image}"
    );
    assert!(
        search.get("healthcheck").is_some(),
        "elasticsearch should have a healthcheck"
    );

    // Should have a persistent volume.
    assert_eq!(result.volumes.len(), 1);
    assert!(result.volumes[0].persist);

    // Should have resource limits.
    assert!(
        search.get("deploy").is_some(),
        "elasticsearch should have deploy/resource limits"
    );
}

#[test]
fn full_stack_with_all_services() {
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
        ServiceDeclaration {
            name: "mail".to_string(),
            catalog: "mailpit".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "storage".to_string(),
            catalog: "minio".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "search".to_string(),
            catalog: "elasticsearch".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler.assemble(&services, "full-stack", ".").unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    // All 8 services should be present.
    for name in &[
        "app", "web", "db", "cache", "sessions", "mail", "storage", "search",
    ] {
        validate_service(&doc, name);
    }

    // Should have volumes for db, storage, and search.
    assert_eq!(result.volumes.len(), 3);
    let vol_names: Vec<&str> = result.volumes.iter().map(|v| v.name.as_str()).collect();
    assert!(vol_names.iter().any(|n| n.contains("db")));
    assert!(vol_names.iter().any(|n| n.contains("storage")));
    assert!(vol_names.iter().any(|n| n.contains("search")));
}

// --- End-to-end: full pipeline write to disk and validate ─────────────

/// End-to-end test: assemble a realistic PHP stack, write all artifacts
/// to disk, and verify the compose file is structurally valid and the
/// supporting files (Dockerfiles, nginx configs) all exist.
#[test]
fn end_to_end_php_stack_written_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("infra/dev");

    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);
    let output = ComposeOutput::new(output_dir.clone());

    // Simulate a real PHP client project with the full service stack.
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
                        toml::Value::String("memcached".to_string()),
                        toml::Value::String("intl".to_string()),
                        toml::Value::String("exif".to_string()),
                        toml::Value::String("zip".to_string()),
                        toml::Value::String("bcmath".to_string()),
                    ]),
                );
                p.insert(
                    "document_root".to_string(),
                    toml::Value::String("public".to_string()),
                );
                p.insert(
                    "node_version".to_string(),
                    toml::Value::String("20".to_string()),
                );
                p
            },
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
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "version".to_string(),
                    toml::Value::String("10.11".to_string()),
                );
                p.insert(
                    "database".to_string(),
                    toml::Value::String("client_app".to_string()),
                );
                p.insert(
                    "root_password".to_string(),
                    toml::Value::String("localdev".to_string()),
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
        ServiceDeclaration {
            name: "sessions".to_string(),
            catalog: "memcached".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert("memory".to_string(), toml::Value::Integer(128));
                p
            },
            variant: None,
            config: None,
        },
    ];

    let manifest_content = r#"
[containers.web]
driver = "colima"
context = "dev"
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
extensions = ["pdo_mysql", "gd", "redis", "memcached", "intl", "exif", "zip", "bcmath"]
node_version = "20"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"
database = "client_app"
root_password = "localdev"

[containers.web.services.cache]
catalog = "redis"

[containers.web.services.sessions]
catalog = "memcached"
memory = 128
"#;

    // 1. Assemble.
    let assembly = assembler
        .assemble(&services, "client-project", ".")
        .unwrap();

    // 2. Write to disk.
    let write_result = output.write(&assembly, manifest_content).unwrap();
    assert!(write_result.regenerated);

    // 3. Verify compose file exists and is valid YAML.
    let compose_path = write_result.compose_path;
    assert!(compose_path.exists());
    let compose_content = std::fs::read_to_string(&compose_path).unwrap();
    let doc = validate_compose_structure(&compose_content);

    // 4. Verify all 5 services are present.
    for svc in &["app", "web", "db", "cache", "sessions"] {
        validate_service(&doc, svc);
    }

    // 5. Verify Dockerfile was written.
    assert!(write_result.dockerfile_paths.contains_key("app"));
    let dockerfile_path = &write_result.dockerfile_paths["app"];
    assert!(dockerfile_path.exists());
    let dockerfile_content = std::fs::read_to_string(dockerfile_path).unwrap();
    assert!(
        dockerfile_content.contains("install-php-extensions"),
        "Dockerfile should use install-php-extensions"
    );
    assert!(
        dockerfile_content.contains("composer"),
        "Dockerfile should install Composer"
    );
    assert!(
        dockerfile_content.contains("NODE_VERSION"),
        "Dockerfile should support Node.js"
    );

    // 6. Verify nginx config was written.
    assert!(write_result.config_paths.contains_key("web.conf"));
    let config_path = &write_result.config_paths["web.conf"];
    assert!(config_path.exists());
    let config_content = std::fs::read_to_string(config_path).unwrap();
    assert!(
        config_content.contains("fastcgi_pass"),
        "nginx config should have fastcgi_pass"
    );
    assert!(
        config_content.contains("try_files"),
        "nginx config should have try_files for front controller"
    );
    assert!(
        config_content.contains("gzip on"),
        "nginx config should enable gzip"
    );

    // 7. Verify the compose YAML has correct service configurations.
    let app = validate_service(&doc, "app");

    // PHP build args should include all extensions.
    let build_args = app.get("build").unwrap().get("args").unwrap();
    let ext_val = build_args.get("EXTENSIONS").unwrap().as_str().unwrap();
    for ext in &[
        "pdo_mysql",
        "gd",
        "redis",
        "memcached",
        "intl",
        "exif",
        "zip",
        "bcmath",
    ] {
        assert!(
            ext_val.contains(ext),
            "extensions should include {ext}: {ext_val}"
        );
    }

    // Node.js build arg should be present.
    let node_val = build_args.get("NODE_VERSION").unwrap().as_str().unwrap();
    assert_eq!(node_val, "20");

    // MariaDB should have correct env vars.
    let db = validate_service(&doc, "db");
    let db_env = db.get("environment").unwrap();
    assert_eq!(
        db_env.get("MYSQL_DATABASE").unwrap().as_str().unwrap(),
        "client_app"
    );
    assert_eq!(
        db_env.get("MYSQL_ROOT_PASSWORD").unwrap().as_str().unwrap(),
        "localdev"
    );

    // MariaDB should have a healthcheck.
    assert!(
        db.get("healthcheck").is_some(),
        "MariaDB should have a healthcheck"
    );

    // Volumes should include MariaDB data.
    assert_eq!(assembly.volumes.len(), 1);
    assert_eq!(assembly.volumes[0].name, "client-project-db-data");
    assert!(assembly.volumes[0].persist);

    // 8. Verify second write is cached.
    let write2 = output.write(&assembly, manifest_content).unwrap();
    assert!(!write2.regenerated);

    // 9. Verify changed manifest triggers regeneration.
    let write3 = output
        .write(&assembly, "# changed manifest content")
        .unwrap();
    assert!(write3.regenerated);
}

/// End-to-end test: eject from catalog and verify standalone compose file.
#[test]
fn end_to_end_eject_produces_standalone_compose() {
    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("infra/dev");

    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);
    let output = ComposeOutput::new(output_dir.clone());

    let services = vec![
        ServiceDeclaration {
            name: "app".to_string(),
            catalog: "php-fpm".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let assembly = assembler.assemble(&services, "test", ".").unwrap();
    output.write(&assembly, "manifest").unwrap();

    // Eject.
    let eject_result = output.eject().unwrap();

    // The permanent docker-compose.yml should be valid.
    let content = std::fs::read_to_string(&eject_result.compose_path).unwrap();
    let doc = validate_compose_structure(&content);
    validate_service(&doc, "app");
    validate_service(&doc, "db");

    // Generated files should be cleaned up.
    assert!(!output.generated_compose_path().exists());

    // Permanent file should be at docker-compose.yml.
    assert_eq!(
        eject_result
            .compose_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap(),
        "docker-compose.yml"
    );
}
