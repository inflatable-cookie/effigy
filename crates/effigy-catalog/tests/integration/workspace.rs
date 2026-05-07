use std::collections::HashMap;

use effigy_catalog::assembly::{ComposeAssembler, ServiceDeclaration};
use effigy_catalog::output::ComposeOutput;

use super::{bundled_resolver, validate_compose_structure, validate_service};

// --- workspace-rust-bun fragment ──────────────────────────────────────

#[test]
fn resolve_workspace_rust_bun_fragment() {
    let resolver = bundled_resolver();
    let fragment = resolver.resolve("workspace-rust-bun").unwrap();

    assert_eq!(fragment.name, "workspace-rust-bun");
    assert_eq!(fragment.schema.service.name, "workspace-rust-bun");
    assert!(fragment.dockerfile.is_some(), "should ship a Dockerfile");
    assert!(!fragment.compose_template.is_empty());
    assert!(fragment.schema.capabilities.exec_target);
    assert_eq!(
        fragment.schema.capabilities.shell.as_deref(),
        Some("/bin/bash")
    );

    // Generic cargo caches should be declared as persistent named volumes.
    let cargo_registry = fragment
        .schema
        .volumes
        .get("cargo-registry")
        .expect("cargo-registry volume declared");
    assert_eq!(cargo_registry.mount, "/usr/local/cargo/registry");
    assert!(cargo_registry.persist);
    let cargo_git = fragment
        .schema
        .volumes
        .get("cargo-git")
        .expect("cargo-git volume declared");
    assert_eq!(cargo_git.mount, "/usr/local/cargo/git");
    assert!(cargo_git.persist);

    // Dockerfile should carry the rust+bun install shape.
    let dockerfile = fragment.dockerfile.as_deref().unwrap();
    assert!(
        dockerfile.contains("FROM rust:${RUST_VERSION}-bookworm"),
        "Dockerfile should build on the pinned Rust base image"
    );
    assert!(
        dockerfile.contains("https://bun.sh/install"),
        "Dockerfile should install Bun"
    );
    assert!(
        dockerfile.contains("useradd --uid \"${WORKSPACE_UID}\""),
        "Dockerfile should create a non-root `dev` user aligned with host UID/GID"
    );
}

#[test]
fn workspace_rust_bun_assembles_with_defaults() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "workspace".to_string(),
        catalog: "workspace-rust-bun".to_string(),
        params: HashMap::new(),
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(&services, "example-dev", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

    let doc = validate_compose_structure(&result.compose_yaml);
    let workspace = validate_service(&doc, "workspace");

    // Build block with pinned arg values for UID/GID.
    let build = workspace.get("build").expect("build block");
    let args = build.get("args").expect("build args");
    assert_eq!(args.get("WORKSPACE_UID").unwrap().as_str().unwrap(), "1000");
    assert_eq!(args.get("WORKSPACE_GID").unwrap().as_str().unwrap(), "1000");
    assert_eq!(args.get("RUST_VERSION").unwrap().as_str().unwrap(), "1.91");

    // sleep infinity command (long-running shell target, not a real service).
    let command = workspace.get("command").expect("command");
    let command_debug = format!("{command:?}");
    assert!(
        command_debug.contains("sleep") && command_debug.contains("infinity"),
        "command should be sleep infinity, got: {command_debug}"
    );

    // Default working_dir is workspace_mount itself.
    assert_eq!(
        workspace.get("working_dir").unwrap().as_str().unwrap(),
        "/workspace-root"
    );

    // Healthcheck proves toolchain availability.
    let health = workspace.get("healthcheck").expect("healthcheck");
    let test = format!("{:?}", health.get("test").unwrap());
    assert!(
        test.contains("cargo") && test.contains("bun"),
        "healthcheck should verify cargo and bun are present, got: {test}"
    );

    // No ports published by default.
    assert!(
        workspace.get("ports").is_none(),
        "no host ports should be published when host_ports param is empty"
    );

    // Cargo caches declared as named volumes.
    let vol_names: Vec<&str> = result.volumes.iter().map(|v| v.name.as_str()).collect();
    assert!(vol_names.contains(&"example-dev-workspace-cargo-registry"));
    assert!(vol_names.contains(&"example-dev-workspace-cargo-git"));

    // Dockerfile carried alongside assembly output.
    assert!(result.dockerfiles.contains_key("workspace"));
}

#[test]
fn workspace_rust_bun_publishes_host_ports_when_requested() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "workspace".to_string(),
        catalog: "workspace-rust-bun".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "host_ports".to_string(),
                toml::Value::Array(vec![
                    toml::Value::String("41001:41001".to_string()),
                    toml::Value::String("41002:41002".to_string()),
                    toml::Value::String("41003:41003".to_string()),
                ]),
            );
            p.insert(
                "working_subdir".to_string(),
                toml::Value::String("underlay-reference".to_string()),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(
            &services,
            "underlay-dev",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    let doc = validate_compose_structure(&result.compose_yaml);
    let workspace = validate_service(&doc, "workspace");

    let ports = workspace
        .get("ports")
        .expect("host_ports should materialise in compose")
        .as_sequence()
        .expect("ports should be a sequence");
    let port_strings: Vec<&str> = ports.iter().filter_map(|p| p.as_str()).collect();
    assert_eq!(
        port_strings,
        vec!["41001:41001", "41002:41002", "41003:41003"]
    );

    // working_subdir shapes the compose working_dir.
    assert_eq!(
        workspace.get("working_dir").unwrap().as_str().unwrap(),
        "/workspace-root/underlay-reference"
    );
}

#[test]
fn workspace_rust_bun_emits_named_volumes_for_subproject_dirs() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "workspace".to_string(),
        catalog: "workspace-rust-bun".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert(
                "working_subdir".to_string(),
                toml::Value::String("underlay-reference".to_string()),
            );
            p.insert(
                "isolated_dirs".to_string(),
                toml::Value::Array(vec![
                    toml::Value::String("acme-api/target".to_string()),
                    toml::Value::String("acme-client/node_modules".to_string()),
                    toml::Value::String("acme-ui/node_modules".to_string()),
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
            "underlay-reference-dev",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    let doc = validate_compose_structure(&result.compose_yaml);
    let workspace = validate_service(&doc, "workspace");

    let volumes = workspace
        .get("volumes")
        .expect("workspace volumes")
        .as_sequence()
        .expect("volumes sequence");
    let volume_strings: Vec<&str> = volumes.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        volume_strings.contains(
            &"underlay-reference-dev-workspace-workspace-root-underlay-reference-acme-api-target:/workspace-root/underlay-reference/acme-api/target"
        ),
        "expected isolated target volume mount; got {volume_strings:?}"
    );
    assert!(
        volume_strings.contains(
            &"underlay-reference-dev-workspace-workspace-root-underlay-reference-acme-client-node-modules:/workspace-root/underlay-reference/acme-client/node_modules"
        ),
        "expected isolated node_modules volume mount; got {volume_strings:?}"
    );
    assert!(
        volume_strings.contains(
            &"underlay-reference-dev-workspace-workspace-root-underlay-reference-acme-ui-node-modules:/workspace-root/underlay-reference/acme-ui/node_modules"
        ),
        "expected isolated node_modules volume mount; got {volume_strings:?}"
    );

    // Top-level named volumes should include the isolated-dir ones, so
    // compose actually creates persistent volumes rather than anonymous ones.
    let vol_names: Vec<&str> = result.volumes.iter().map(|v| v.name.as_str()).collect();
    assert!(vol_names.contains(&"underlay-reference-dev-workspace-cargo-registry"));
    assert!(vol_names.contains(&"underlay-reference-dev-workspace-cargo-git"));
    assert!(vol_names.contains(
        &"underlay-reference-dev-workspace-workspace-root-underlay-reference-acme-api-target"
    ));
    assert!(vol_names.contains(&"underlay-reference-dev-workspace-workspace-root-underlay-reference-acme-client-node-modules"));
    assert!(vol_names.contains(
        &"underlay-reference-dev-workspace-workspace-root-underlay-reference-acme-ui-node-modules"
    ));
}

#[test]
fn underlay_style_stack_assembles_with_bundled_fragments_only() {
    // This is the proof against underlay-reference: the exact service shape
    // that repo's `infra/dev/docker-compose.yml` + `workspace.Dockerfile`
    // carry today can be expressed with only bundled catalog fragments.
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "workspace".to_string(),
            catalog: "workspace-rust-bun".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "working_subdir".to_string(),
                    toml::Value::String("underlay-reference".to_string()),
                );
                p.insert(
                    "host_ports".to_string(),
                    toml::Value::Array(vec![
                        toml::Value::String("41001:41001".to_string()),
                        toml::Value::String("41002:41002".to_string()),
                        toml::Value::String("41003:41003".to_string()),
                    ]),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "postgres".to_string(),
            catalog: "postgres".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "database".to_string(),
                    toml::Value::String("acme".to_string()),
                );
                p.insert(
                    "password".to_string(),
                    toml::Value::String("postgres".to_string()),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "dbgate".to_string(),
            catalog: "dbgate".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "database".to_string(),
                    toml::Value::String("acme".to_string()),
                );
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "mailpit".to_string(),
            catalog: "mailpit".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "minio".to_string(),
            catalog: "minio".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(
            &services,
            "underlay-reference-dev",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    for name in &["workspace", "postgres", "dbgate", "mailpit", "minio"] {
        validate_service(&doc, name);
    }

    // workspace must publish the repo-owned dev server ports.
    let workspace = validate_service(&doc, "workspace");
    let workspace_ports: Vec<&str> = workspace
        .get("ports")
        .unwrap()
        .as_sequence()
        .unwrap()
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    assert!(workspace_ports.contains(&"41001:41001"));
    assert!(workspace_ports.contains(&"41002:41002"));
    assert!(workspace_ports.contains(&"41003:41003"));

    // postgres comes up with the underlay DB name and pinned password.
    let postgres = validate_service(&doc, "postgres");
    let pg_env = postgres.get("environment").unwrap();
    assert_eq!(pg_env.get("POSTGRES_DB").unwrap().as_str().unwrap(), "acme");
    assert_eq!(
        pg_env.get("POSTGRES_PASSWORD").unwrap().as_str().unwrap(),
        "postgres"
    );
    // dbgate inherits the bundled postgres database + password through the
    // fragment's services-map lookup.
    let dbgate = validate_service(&doc, "dbgate");
    let dbgate_env = dbgate
        .get("environment")
        .expect("dbgate service declares environment block");
    assert_eq!(
        dbgate_env.get("SERVER_pg").unwrap().as_str().unwrap(),
        "postgres"
    );
    assert_eq!(
        dbgate_env.get("DATABASE_pg").unwrap().as_str().unwrap(),
        "acme"
    );
    assert_eq!(
        dbgate_env.get("PASSWORD_pg").unwrap().as_str().unwrap(),
        "postgres"
    );

    // Named volumes expected: cargo-registry, cargo-git, Postgres data, minio
    // data, and dbgate settings. Mailpit has no persistent volume.
    let vol_names: std::collections::BTreeSet<&str> =
        result.volumes.iter().map(|v| v.name.as_str()).collect();
    for expected in &[
        "underlay-reference-dev-workspace-cargo-registry",
        "underlay-reference-dev-workspace-cargo-git",
        "underlay-reference-dev-postgres-data",
        "underlay-reference-dev-minio-data",
        "underlay-reference-dev-dbgate-data",
    ] {
        assert!(
            vol_names.contains(expected),
            "missing expected volume `{expected}`; got {vol_names:?}"
        );
    }

    // Dockerfile for the workspace service must be carried through to the
    // assembly output so the runtime can write it next to the compose file.
    assert!(result.dockerfiles.contains_key("workspace"));
    let dockerfile = &result.dockerfiles["workspace"];
    assert!(dockerfile.contains("FROM rust:"));
    assert!(dockerfile.contains("bun.sh/install"));
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
                    toml::Value::String(".".to_string()),
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
            params: {
                let mut p = HashMap::new();
                p.insert(
                    "document_root".to_string(),
                    toml::Value::String(".".to_string()),
                );
                p.insert(
                    "rewrite_all_to".to_string(),
                    toml::Value::String("/vendor/genesis.php".to_string()),
                );
                p.insert(
                    "asset_fallback".to_string(),
                    toml::Value::String(String::new()),
                );
                p.insert(
                    "error_page_404".to_string(),
                    toml::Value::String("/vendor/genesis.php".to_string()),
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
                p.insert(
                    "database".to_string(),
                    toml::Value::String("client_app".to_string()),
                );
                p.insert(
                    "password".to_string(),
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
document_root = "."
node_version = "20"

[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = ""
error_page_404 = "/vendor/genesis.php"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"
database = "client_app"
password = "localdev"

[containers.web.services.cache]
catalog = "redis"

[containers.web.services.sessions]
catalog = "memcached"
memory = 128
"#;

    // 1. Assemble.
    let assembly = assembler
        .assemble(
            &services,
            "client-project",
            ".",
            ".effigy-catalog",
            1000,
            1000,
        )
        .unwrap();

    // 2. Write to disk.
    let write_result = output.write(&assembly, manifest_content).unwrap();
    assert!(write_result.regenerated);

    // 3. Verify compose file exists and is valid YAML.
    let compose_path = write_result.compose_path;
    assert!(compose_path.exists());
    let compose_content = std::fs::read_to_string(&compose_path).unwrap();
    let doc = validate_compose_structure(&compose_content);
    let app = validate_service(&doc, "app");
    let app_volumes = app.get("volumes").unwrap().as_sequence().unwrap();
    let app_volume_strings: Vec<&str> = app_volumes
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        app_volume_strings
            .iter()
            .any(|value| value.ends_with(":/var/www/html")),
        "php app should mount the repo at the workspace working dir"
    );
    let web = validate_service(&doc, "web");
    let web_volumes = web.get("volumes").unwrap().as_sequence().unwrap();
    let web_volume_strings: Vec<&str> = web_volumes
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert!(
        web_volume_strings
            .iter()
            .any(|value| value.ends_with(":/var/www/html:ro")),
        "nginx should mount the repo read-only at the workspace working dir"
    );

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
        dockerfile_content.contains("COPY --from=mlocati/php-extension-installer:latest"),
        "Dockerfile should copy install-php-extensions from the published installer image"
    );
    assert!(
        !dockerfile_content.contains(
            "https://github.com/mlocati/docker-php-extension-installer/releases/latest/download/install-php-extensions"
        ),
        "Dockerfile should not depend on the live GitHub release download URL at build time"
    );
    assert!(
        dockerfile_content.contains("composer"),
        "Dockerfile should install Composer"
    );
    assert!(
        dockerfile_content.contains("NODE_VERSION"),
        "Dockerfile should support Node.js"
    );
    assert!(
        dockerfile_content.contains("user = dev"),
        "Dockerfile should run php-fpm workers as dev"
    );
    assert!(
        dockerfile_content.contains("id -gn dev"),
        "Dockerfile should derive the php-fpm group from the dev user's primary group"
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
        config_content.contains("rewrite .* /vendor/genesis.php last;"),
        "nginx config should rewrite through vendor/genesis.php"
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

    let assembly = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
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
