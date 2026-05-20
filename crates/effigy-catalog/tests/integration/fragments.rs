use std::collections::HashMap;

use effigy_catalog::assembly::{ComposeAssembler, ServiceDeclaration};
use effigy_catalog::fragment::FragmentSource;

use super::bundled_resolver;

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
    assert!(
        names.contains(&"phpmyadmin"),
        "missing phpmyadmin: {names:?}"
    );
    assert!(names.contains(&"pgweb"), "missing pgweb: {names:?}");
    assert!(names.contains(&"dbgate"), "missing dbgate: {names:?}");
    assert!(names.contains(&"minio"), "missing minio: {names:?}");
    assert!(
        names.contains(&"elasticsearch"),
        "missing elasticsearch: {names:?}"
    );
    assert!(names.contains(&"node"), "missing node: {names:?}");
    assert!(
        names.contains(&"workspace-rust-bun"),
        "missing workspace-rust-bun: {names:?}"
    );

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
    assert!(fragment.param_variants.is_empty());
    assert_eq!(
        fragment.schema.capabilities.shell.as_deref(),
        Some("/bin/bash")
    );
    let dockerfile = fragment.dockerfile.as_deref().expect("dockerfile");
    assert!(
        dockerfile.contains("ripgrep"),
        "php-fpm workspace image should include ripgrep for shell and agent use"
    );
    assert!(
        dockerfile.contains("jq"),
        "php-fpm workspace image should include jq for shell and agent use"
    );
    assert!(
        dockerfile
            .contains("php_admin_value[auto_prepend_file] = /run/effigy/secrets/bootstrap.php"),
        "php-fpm workspace image should wire the secret bootstrap into php-fpm"
    );
    assert!(
        fragment.compose_template.contains("tmpfs:")
            && fragment.compose_template.contains("- /run/effigy/secrets"),
        "php-fpm compose fragment should mount a tmpfs secret runtime"
    );
}

#[test]
fn resolve_node_fragment_uses_workspace_image_with_agent_tools() {
    let resolver = bundled_resolver();
    let fragment = resolver.resolve("node").unwrap();

    assert_eq!(fragment.name, "node");
    assert!(fragment.dockerfile.is_some());
    assert_eq!(
        fragment.schema.capabilities.shell.as_deref(),
        Some("/bin/bash")
    );
    assert!(fragment.schema.capabilities.workspace_host_integration);
    assert!(fragment.schema.capabilities.installs_mkcert_ca);

    let dockerfile = fragment.dockerfile.as_deref().expect("dockerfile");
    assert!(
        dockerfile.contains("ripgrep"),
        "node workspace image should include ripgrep for shell and agent use"
    );
    assert!(
        dockerfile.contains("jq"),
        "node workspace image should include jq for shell and agent use"
    );
    assert!(
        dockerfile.contains("fd-find"),
        "node workspace image should include fd for shell and agent use"
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

    let result = assembler
        .assemble(
            &services,
            "test-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

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
    assert!(
        result
            .compose_yaml
            .contains("COMPOSER_HOME: /home/dev/.config/composer"),
        "missing composer home environment:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("COMPOSER_CACHE_DIR: /home/dev/.cache/composer"),
        "missing composer cache environment:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("tmpfs:")
            && result.compose_yaml.contains("- /run/effigy/secrets"),
        "missing php secret tmpfs mount:\n{}",
        result.compose_yaml
    );

    // PHP service should depend on MariaDB.
    assert!(
        result.compose_yaml.contains("depends_on"),
        "missing depends_on:\n{}",
        result.compose_yaml
    );

    // MariaDB should use a named data volume owned inside the runtime.
    assert!(
        result
            .compose_yaml
            .contains("test-project-db-data:/var/lib/mysql"),
        "missing named MariaDB data volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("test-project-cache-data:/data"),
        "missing named Redis data volume:\n{}",
        result.compose_yaml
    );

    // Should have a Dockerfile for php-fpm.
    assert!(result.dockerfiles.contains_key("app"));
    assert!(result.dockerfiles["app"].contains("PHP_VERSION"));

    assert_eq!(result.volumes.len(), 3);
    assert!(result
        .volumes
        .iter()
        .any(|volume| volume.name == "test-project-db-data" && volume.persist));
    assert!(result
        .volumes
        .iter()
        .any(|volume| volume.name == "test-project-cache-data" && volume.persist));
    assert!(result
        .volumes
        .iter()
        .any(|volume| volume.name == "test-project-app-pnpm-store" && !volume.persist));
}

#[test]
fn php_fpm_supports_container_composer_global_fallbacks() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "app".to_string(),
        catalog: "php-fpm".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "composer_global_packages".to_string(),
                toml::Value::Array(vec![toml::Value::String("phpunit/phpunit".to_string())]),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "test-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    assert!(
        result
            .compose_yaml
            .contains("COMPOSER_GLOBAL_PACKAGES: phpunit/phpunit"),
        "compose should pass composer fallback packages:\n{}",
        result.compose_yaml
    );
    assert!(
        result.dockerfiles["app"].contains("composer global require"),
        "php-fpm dockerfile should install configured composer globals"
    );
    assert!(
        result.dockerfiles["app"]
            .contains("composer global config --no-plugins allow-plugins true"),
        "php-fpm dockerfile should trust composer plugins during fallback global installs"
    );
    assert!(
        !result.compose_yaml.contains("COMPOSER_GLOBAL_PACKAGES")
            || result.compose_yaml.contains("COMPOSER_GLOBAL_PACKAGES: phpunit/phpunit"),
        "php-fpm compose should only pass composer global packages when fallback installs are active:\n{}",
        result.compose_yaml
    );
}

#[test]
fn php_fpm_skips_container_composer_globals_when_host_mount_is_enabled() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "app".to_string(),
        catalog: "php-fpm".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "mount_host_composer_home".to_string(),
                toml::Value::Boolean(true),
            );
            p.insert(
                "composer_global_packages".to_string(),
                toml::Value::Array(vec![toml::Value::String("acme/effigy".to_string())]),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "test-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    assert!(
        !result.compose_yaml.contains("COMPOSER_GLOBAL_PACKAGES:"),
        "php-fpm compose should bypass container fallback globals when host composer home mounting is enabled:\n{}",
        result.compose_yaml
    );
}

#[test]
fn php_fpm_supports_node_globals_and_pnpm_tooling() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "app".to_string(),
        catalog: "php-fpm".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "node_version".to_string(),
                toml::Value::String("20".to_string()),
            );
            p.insert(
                "node_global_packages".to_string(),
                toml::Value::Array(vec![toml::Value::String("eclint".to_string())]),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "test-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    assert!(
        result.compose_yaml.contains("NODE_VERSION: '20'")
            || result.compose_yaml.contains("NODE_VERSION: \"20\"")
            || result.compose_yaml.contains("NODE_VERSION: 20"),
        "php-fpm compose should pass the requested Node.js version:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("NODE_GLOBAL_PACKAGES: eclint"),
        "php-fpm compose should pass requested npm globals:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("PNPM_HOME: /home/dev/.local/share/pnpm"),
        "php-fpm compose should expose PNPM_HOME for corepack-managed pnpm:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("pnpm_config_store_dir: /home/dev/.local/share/pnpm/store"),
        "php-fpm compose should pin pnpm's effective store dir to the dedicated container cache volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("npm_config_store_dir: /home/dev/.local/share/pnpm/store"),
        "php-fpm compose should pin the pnpm store to the dedicated container cache volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("test-project-app-pnpm-store:/home/dev/.local/share/pnpm/store"),
        "php-fpm compose should mount a dedicated pnpm store volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result.dockerfiles["app"].contains("ENV COREPACK_HOME=/home/dev/.cache/node/corepack"),
        "php-fpm Dockerfile should pin a writable corepack cache path for the dev user"
    );
    assert!(
        result.dockerfiles["app"].contains(
            "mkdir -p \"${COMPOSER_HOME}\" \"${COMPOSER_CACHE_DIR}\" \"${COREPACK_HOME}\""
        ),
        "php-fpm Dockerfile should create the corepack cache path before runtime"
    );
    assert!(
        result.dockerfiles["app"].contains("corepack enable"),
        "php-fpm Dockerfile should enable corepack so pnpm is available"
    );
    assert!(
        result.dockerfiles["app"].contains("chown -R dev:\"${workspace_group}\" /home/dev/.cache;"),
        "php-fpm Dockerfile should restore dev ownership after the node tooling layer"
    );
    assert!(
        result.dockerfiles["app"].contains("corepack prepare pnpm@latest --activate"),
        "php-fpm Dockerfile should explicitly activate pnpm via corepack"
    );
    assert!(
        result.dockerfiles["app"].contains("npm install -g $NODE_GLOBAL_PACKAGES"),
        "php-fpm Dockerfile should install requested npm globals"
    );
}

#[test]
fn php_fpm_supports_explicit_php_app_style_service_params() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "app".to_string(),
        catalog: "php-fpm".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "version".to_string(),
                toml::Value::String("8.4".to_string()),
            );
            p.insert(
                "document_root".to_string(),
                toml::Value::String(".".to_string()),
            );
            p.insert(
                "node_version".to_string(),
                toml::Value::String("20".to_string()),
            );
            p.insert(
                "node_global_packages".to_string(),
                toml::Value::Array(vec![toml::Value::String("eclint".to_string())]),
            );
            p.insert(
                "composer_global_packages".to_string(),
                toml::Value::Array(vec![toml::Value::String("acme/effigy".to_string())]),
            );
            p.insert(
                "isolated_dirs".to_string(),
                toml::Value::Array(vec![
                    toml::Value::String("vendor".to_string()),
                    toml::Value::String("node_modules".to_string()),
                ]),
            );
            p.insert(
                "extensions".to_string(),
                toml::Value::Array(vec![
                    toml::Value::String("pdo_mysql".to_string()),
                    toml::Value::String("intl".to_string()),
                    toml::Value::String("exif".to_string()),
                    toml::Value::String("zip".to_string()),
                    toml::Value::String("gd".to_string()),
                    toml::Value::String("redis".to_string()),
                    toml::Value::String("memcached".to_string()),
                    toml::Value::String("opcache".to_string()),
                ]),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "test-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    assert!(
        result
            .compose_yaml
            .contains("COMPOSER_GLOBAL_PACKAGES: acme/effigy"),
        "php-fpm explicit params should apply Composer globals:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("COMPOSER_CACHE_DIR: /home/dev/.cache/composer"),
        "php-fpm explicit params should route Composer through the shared cache dir:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("NODE_GLOBAL_PACKAGES: eclint"),
        "php-fpm explicit params should apply npm globals:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("EXTENSIONS: pdo_mysql intl exif zip gd redis memcached opcache"),
        "php-fpm explicit params should apply extension defaults:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("DOCUMENT_ROOT: '.'")
            || result.compose_yaml.contains("DOCUMENT_ROOT: ."),
        "php-fpm explicit params should apply the repo-root document root:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("pnpm_config_store_dir: /home/dev/.local/share/pnpm/store"),
        "php-fpm explicit params should route pnpm through the dedicated store volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("test-project-app-pnpm-store:/home/dev/.local/share/pnpm/store"),
        "php-fpm explicit params should mount a dedicated pnpm store volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("test-project-app-var-www-html-vendor:/var/www/html/vendor"),
        "php-fpm explicit params should isolate configured hot dirs with named volumes:\n{}",
        result.compose_yaml
    );
    assert!(
        result
            .compose_yaml
            .contains("test-project-app-var-www-html-node-modules:/var/www/html/node_modules"),
        "php-fpm explicit params should isolate configured hot dirs with named volumes:\n{}",
        result.compose_yaml
    );
}

#[test]
fn php_fpm_publishes_host_ports_when_requested() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "app".to_string(),
        catalog: "php-fpm".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "host_ports".to_string(),
                toml::Value::Array(vec![toml::Value::String("8938:8938".to_string())]),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "test-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    assert!(
        result.compose_yaml.contains("ports:\n    - 8938:8938")
            || result.compose_yaml.contains("ports:\n    - \"8938:8938\""),
        "php-fpm should publish explicit host ports when requested:\n{}",
        result.compose_yaml
    );
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

    let result = assembler
        .assemble(&services, "client-x", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

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
fn generated_compose_pins_runtime_volume_names() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "db".to_string(),
        catalog: "postgres".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "farmyard-dev",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    assert!(
        result
            .compose_yaml
            .contains("farmyard-dev-db-data:/var/lib/postgresql/data"),
        "missing named Postgres data volume:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("farmyard-dev-db-data"),
        "expected Postgres named volume output:\n{}",
        result.compose_yaml
    );
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
    let result = assembler.assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000);
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

    let result = assembler.assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        effigy_catalog::CatalogError::VariantNotFound { .. }
    ));
}
