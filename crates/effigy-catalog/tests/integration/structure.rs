use std::collections::HashMap;

use effigy_catalog::assembly::{ComposeAssembler, ServiceDeclaration};
use effigy_catalog::fragment::{CatalogResolver, FragmentSource};
use effigy_catalog::output::ComposeOutput;

use super::{bundled_resolver, validate_compose_structure, validate_service};

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
    let assembly = assembler
        .assemble(&services, "test-proj", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

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
                    toml::Value::String(".".to_string()),
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
                    "databases".to_string(),
                    toml::Value::Array(vec![
                        toml::Value::String("clientdb".to_string()),
                        toml::Value::String("clientdb_test".to_string()),
                    ]),
                );
                p.insert(
                    "password".to_string(),
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
        .assemble(
            &services,
            "client-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
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
    let app_volumes = app.get("volumes").unwrap().as_sequence().unwrap();
    let app_volume_strings: Vec<&str> = app_volumes
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        app_volume_strings.contains(&".:/var/www/html"),
        "app should mount the repo at the workspace working dir"
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
    let web_volumes = web.get("volumes").unwrap().as_sequence().unwrap();
    let web_volume_strings: Vec<&str> = web_volumes
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        web_volume_strings.contains(&".:/var/www/html:ro"),
        "web should mount the repo read-only at the workspace working dir"
    );

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
    let db_volumes = db.get("volumes").unwrap().as_sequence().unwrap();
    let db_volume_strings: Vec<&str> = db_volumes
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        db_volume_strings
            .contains(&"./.effigy/runtime/compose/db.conf:/docker-entrypoint-initdb.d/10-extra-databases.sql:ro"),
        "mariadb should mount an init script for extra databases: {db_volume_strings:?}"
    );

    let cache = validate_service(&doc, "cache");
    let cache_image = cache.get("image").unwrap().as_str().unwrap();
    assert!(cache_image.contains("redis"));

    let sessions = validate_service(&doc, "sessions");
    let sessions_cmd = sessions.get("command").unwrap();
    // Memcached command should include the memory flag.
    let cmd_str = format!("{sessions_cmd:?}");
    assert!(cmd_str.contains("128"), "memcached should use 128MB memory");

    // 3. Validate persistent database storage uses a named volume so the
    // database image owns filesystem permissions inside the runtime.
    assert!(
        result
            .compose_yaml
            .contains("client-project-db-data:/var/lib/mysql"),
        "should have a named mariadb data volume:\n{}",
        result.compose_yaml
    );
    let volumes = doc
        .get("volumes")
        .and_then(|value| value.as_mapping())
        .expect("volumes");
    assert!(
        volumes
            .keys()
            .any(|k| { k.as_str().map(|s| s.contains("db-data")).unwrap_or(false) }),
        "mariadb should emit a named data volume"
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
        result.dockerfiles["app"].contains("user = dev"),
        "Dockerfile should reconfigure php-fpm workers onto the dev user"
    );
    assert!(
        result.dockerfiles["app"].contains("id -gn dev"),
        "Dockerfile should derive the php-fpm group from the dev user's primary group"
    );
    assert!(
        result.dockerfiles["app"].contains("opcache.enable = 1"),
        "Dockerfile should explicitly enable opcache for php-fpm:\n{}",
        result.dockerfiles["app"]
    );
    assert!(
        result.dockerfiles["app"].contains("realpath_cache_size = 4096K"),
        "Dockerfile should explicitly tune PHP realpath cache for php-fpm:\n{}",
        result.dockerfiles["app"]
    );
    assert!(
        result.dockerfiles["app"].contains("short_open_tag = Off"),
        "Dockerfile should explicitly disable short_open_tag for php-fpm:\n{}",
        result.dockerfiles["app"]
    );
    assert!(
        result.dockerfiles["app"].contains("opcache.revalidate_freq = 1"),
        "Dockerfile should explicitly tune opcache revalidation for dev:\n{}",
        result.dockerfiles["app"]
    );

    assert!(
        result.config_files.contains_key("web.conf"),
        "should have nginx config"
    );
    assert!(
        result.config_files["web.conf"].contains("fastcgi_pass"),
        "nginx config should have fastcgi_pass"
    );
    assert!(
        result.config_files.contains_key("db.conf"),
        "should have mariadb init config"
    );
    assert!(
        result.config_files["db.conf"].contains("clientdb_test"),
        "mariadb init config should create the extra database"
    );

    // 5. Validate volume metadata.
    assert_eq!(result.volumes.len(), 3);
    assert!(result
        .volumes
        .iter()
        .any(|volume| volume.name == "client-project-db-data" && volume.persist));
    assert!(result
        .volumes
        .iter()
        .any(|volume| volume.name == "client-project-app-pnpm-store" && !volume.persist));
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

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
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
                    "databases".to_string(),
                    toml::Value::Array(vec![
                        toml::Value::String("myapp".to_string()),
                        toml::Value::String("myapp_test".to_string()),
                    ]),
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

    let result = assembler
        .assemble(&services, "rust-svc", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    let db = validate_service(&doc, "db");
    let db_image = db.get("image").unwrap().as_str().unwrap();
    assert!(db_image.contains("postgres:16"));

    let db_env = db.get("environment").unwrap();
    assert_eq!(
        db_env.get("POSTGRES_DB").unwrap().as_str().unwrap(),
        "myapp"
    );
    let db_volumes = db.get("volumes").unwrap().as_sequence().unwrap();
    let db_volume_strings: Vec<&str> = db_volumes
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        db_volume_strings
            .contains(&"./.effigy/runtime/compose/db.conf:/docker-entrypoint-initdb.d/10-extra-databases.sql:ro"),
        "postgres should mount an init script for extra databases: {db_volume_strings:?}"
    );
    assert!(
        result.config_files.contains_key("db.conf"),
        "should have postgres init config"
    );
    assert!(
        result.config_files["db.conf"].contains("myapp_test"),
        "postgres init config should create the extra database"
    );

    assert!(
        result
            .compose_yaml
            .contains("rust-svc-db-data:/var/lib/postgresql/data"),
        "missing named Postgres data volume:\n{}",
        result.compose_yaml
    );
    assert_eq!(result.volumes.len(), 2);
    assert!(result
        .volumes
        .iter()
        .any(|volume| volume.name == "rust-svc-db-data" && volume.persist));
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

    let result = assembler.assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000);
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

    let result = assembler.assemble(&[], "test", ".", ".effigy-catalog", 1000, 1000);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        effigy_catalog::CatalogError::EmptyServiceList
    ));
}

#[test]
fn single_redis_service_assembles_with_persistent_data_volume() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "cache".to_string(),
        catalog: "redis".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(&services, "minimal", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    validate_service(&doc, "cache");
    assert_eq!(result.volumes.len(), 1);
    assert_eq!(result.volumes[0].name, "minimal-cache-data");
    assert!(result.volumes[0].persist);
    assert!(doc.get("volumes").is_some());
}
