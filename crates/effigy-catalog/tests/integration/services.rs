use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use effigy_catalog::assembly::{ComposeAssembler, ServiceDeclaration};

use super::{bundled_resolver, validate_compose_structure, validate_service};

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

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

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

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

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

#[test]
fn nginx_supports_genesis_rewrite_params_without_variant() {
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
                    toml::Value::String("/vendor/genesis.php".to_string()),
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
    ];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

    let config = &result.config_files["web.conf"];
    assert!(
        config.contains("root /var/www/html;"),
        "nginx root should use the repo root, got:\n{config}"
    );
    assert!(
        config.contains("rewrite .* /vendor/genesis.php last;"),
        "nginx config should rewrite through vendor/genesis.php, got:\n{config}"
    );
    assert!(
        config.contains("try_files $uri /vendor/genesis.php;"),
        "nginx config should route missing assets through vendor/genesis.php, got:\n{config}"
    );
    assert!(
        config.contains("fastcgi_param SERVER_PROTOCOL $server_protocol;"),
        "nginx config should pass SERVER_PROTOCOL through to PHP-FPM, got:\n{config}"
    );
}

#[test]
fn nginx_healthcheck_survives_compose_interpolation_and_treats_http_responses_as_ready() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "web".to_string(),
        catalog: "nginx".to_string(),
        params: HashMap::new(),
        variant: Some("default".to_string()),
        config: None,
    }];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

    let doc = validate_compose_structure(&result.compose_yaml);
    let web = validate_service(&doc, "web");
    let health = web.get("healthcheck").expect("healthcheck");
    let test = health.get("test").expect("healthcheck test");
    let command = nginx_healthcheck_command(test);

    assert!(
        command.contains("wget -q -O /dev/null http://127.0.0.1:80/") && command.contains("eq 8"),
        "nginx healthcheck should treat any HTTP response as ready, got: {command}"
    );
    assert!(
        command.contains("$$code"),
        "rendered health command must use $$ so Compose leaves $code for the shell, got: {command}"
    );
    assert!(
        result.compose_yaml.contains("$$code"),
        "serialized compose must retain $$code for nerdctl/Docker Compose interpolation, got:\n{}",
        result.compose_yaml
    );

    let (interpolated_yaml, yaml_warnings) = compose_interpolate(&result.compose_yaml);
    assert!(
        !yaml_warnings.iter().any(|name| name == "code"),
        "Compose interpolation must not substitute `code`; warnings: {yaml_warnings:?}\n{interpolated_yaml}"
    );

    let (interpolated, warnings) = compose_interpolate(command);
    assert!(
        warnings.is_empty(),
        "health command should not trigger Compose substitution warnings, got {warnings:?} from {command}"
    );
    assert!(
        interpolated.contains("[ \"$code\" -eq 0 ]")
            && interpolated.contains("[ \"$code\" -eq 8 ]"),
        "after Compose interpolation the shell must still see $code, got: {interpolated}"
    );
    assert!(
        !interpolated.contains("[ \"\" -eq"),
        "Compose must not blank the wget exit-code variable, got: {interpolated}"
    );

    assert_eq!(
        nginx_health_status_for_wget_exit(&interpolated, 0),
        0,
        "HTTP success (wget 0) should be healthy"
    );
    assert_eq!(
        nginx_health_status_for_wget_exit(&interpolated, 8),
        0,
        "HTTP error such as 404 (wget 8) should be healthy"
    );
    assert_ne!(
        nginx_health_status_for_wget_exit(&interpolated, 1),
        0,
        "generic wget failure should be unhealthy"
    );
    assert_ne!(
        nginx_health_status_for_wget_exit(&interpolated, 4),
        0,
        "network failure should be unhealthy"
    );
}

fn nginx_healthcheck_command(test: &serde_yaml::Value) -> &str {
    let seq = test.as_sequence().unwrap_or_else(|| {
        panic!("healthcheck test should be a CMD-SHELL sequence, got: {test:?}")
    });
    assert_eq!(
        seq.first().and_then(serde_yaml::Value::as_str),
        Some("CMD-SHELL"),
        "healthcheck test should start with CMD-SHELL, got: {test:?}"
    );
    seq.get(1)
        .and_then(serde_yaml::Value::as_str)
        .unwrap_or_else(|| panic!("healthcheck test missing command string, got: {test:?}"))
}

/// Compose-spec interpolation for `$VAR`, `${VAR}`, and `$$` → `$`.
fn compose_interpolate(input: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut warnings = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if i + 1 < chars.len() && chars[i + 1] == '$' {
            out.push('$');
            i += 2;
            continue;
        }
        if i + 1 < chars.len() && chars[i + 1] == '{' {
            let start = i + 2;
            let mut end = start;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            if end < chars.len() && chars[end] == '}' {
                let name: String = chars[start..end].iter().collect();
                if compose_ident(&name) {
                    warnings.push(name);
                } else {
                    out.push_str(&format!("${{{name}}}"));
                }
                i = end + 1;
                continue;
            }
        }
        if i + 1 < chars.len() && compose_ident_start(chars[i + 1]) {
            let start = i + 1;
            let mut end = start + 1;
            while end < chars.len() && compose_ident_continue(chars[end]) {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            warnings.push(name);
            i = end;
            continue;
        }
        out.push('$');
        i += 1;
    }
    (out, warnings)
}

fn compose_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if compose_ident_start(first) => chars.all(compose_ident_continue),
        _ => false,
    }
}

fn compose_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn compose_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn nginx_health_status_for_wget_exit(command: &str, wget_exit: i32) -> i32 {
    let dir = tempfile::tempdir().unwrap();
    let wget = dir.path().join("wget");
    std::fs::write(&wget, format!("#!/bin/sh\nexit {wget_exit}\n")).unwrap();
    let mut perms = std::fs::metadata(&wget).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wget, perms).unwrap();
    let path = format!(
        "{}:{}",
        dir.path().display(),
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string())
    );
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("PATH", path)
        .status()
        .unwrap_or_else(|error| panic!("failed to run health command `{command}`: {error}"))
        .code()
        .unwrap_or(255)
}

#[test]
fn nginx_genesis_rewrite_params_apply_without_variant() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
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
                toml::Value::String("/vendor/genesis.php".to_string()),
            );
            p.insert(
                "error_page_404".to_string(),
                toml::Value::String("/vendor/genesis.php".to_string()),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

    assert!(
        result.compose_yaml.contains("- .:/var/www/html:ro"),
        "nginx params should apply the repo-root working dir preset:\n{}",
        result.compose_yaml
    );
    assert!(
        result.config_files["web.conf"].contains("root /var/www/html;"),
        "nginx params should apply the repo-root document root preset:\n{}",
        result.config_files["web.conf"]
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

    let result = assembler
        .assemble(&services, "frontend", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let app = validate_service(&doc, "app");

    let build = app.get("build").expect("build");
    let dockerfile = build
        .get("dockerfile")
        .and_then(|value| value.as_str())
        .expect("dockerfile path");
    assert!(
        dockerfile.ends_with("/app/Dockerfile"),
        "node service should build from its workspace Dockerfile, got: {dockerfile}"
    );
    let args = build.get("args").expect("build args");
    assert_eq!(
        args.get("NODE_VERSION").and_then(|value| value.as_str()),
        Some("22")
    );

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
    assert!(
        result.dockerfiles.contains_key("app"),
        "node app should expose a Dockerfile artifact"
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

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let mail = validate_service(&doc, "mail");

    let image = mail.get("image").unwrap().as_str().unwrap();
    assert_eq!(
        image, "axllent/mailpit:v1.30.6",
        "mailpit image should be pinned to a concrete version tag"
    );
    assert!(mail.get("ports").is_some(), "mailpit should expose ports");
    assert!(
        mail.get("healthcheck").is_some(),
        "mailpit should have a healthcheck"
    );
}

#[test]
fn phpmyadmin_fragment_assembles() {
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
            name: "dbadmin".to_string(),
            catalog: "phpmyadmin".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let admin = validate_service(&doc, "dbadmin");

    let image = admin.get("image").unwrap().as_str().unwrap();
    assert_eq!(
        image, "phpmyadmin:5.2.3",
        "phpmyadmin image should be pinned to a concrete version tag"
    );
    assert!(
        result.compose_yaml.contains("PMA_HOST: db"),
        "phpmyadmin should target the db service:\n{}",
        result.compose_yaml
    );
    assert!(
        result.compose_yaml.contains("PMA_PASSWORD: secret"),
        "phpmyadmin should inherit the default mariadb root password:\n{}",
        result.compose_yaml
    );
    assert!(
        admin.get("depends_on").is_some(),
        "phpmyadmin should depend on mariadb"
    );
    assert!(
        admin.get("healthcheck").is_some(),
        "phpmyadmin should have a healthcheck"
    );
}

#[test]
fn phpmyadmin_uses_empty_password_when_db_explicitly_sets_empty_password() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: {
                let mut p = HashMap::new();
                p.insert("password".to_string(), toml::Value::String("".to_string()));
                p
            },
            variant: None,
            config: None,
        },
        ServiceDeclaration {
            name: "dbadmin".to_string(),
            catalog: "phpmyadmin".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

    assert!(
        result.compose_yaml.contains("PMA_PASSWORD: ''"),
        "phpmyadmin should keep an explicitly empty mariadb password empty:\n{}",
        result.compose_yaml
    );
}

#[test]
fn phpmyadmin_inherits_mariadb_password() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
        ServiceDeclaration {
            name: "db".to_string(),
            catalog: "mariadb".to_string(),
            params: {
                let mut p = HashMap::new();
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
            name: "dbadmin".to_string(),
            catalog: "phpmyadmin".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();

    assert!(
        result.compose_yaml.contains("PMA_PASSWORD: localdev"),
        "phpmyadmin should inherit the mariadb root password:\n{}",
        result.compose_yaml
    );
}

#[test]
fn pgweb_fragment_assembles_for_postgres() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
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
            name: "pgweb".to_string(),
            catalog: "pgweb".to_string(),
            params: HashMap::new(),
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let pgweb = validate_service(&doc, "pgweb");

    let image = pgweb.get("image").unwrap().as_str().unwrap();
    assert_eq!(
        image, "sosedoff/pgweb:0.17.0",
        "pgweb image should be pinned to a concrete version tag"
    );
    assert!(
        result
            .compose_yaml
            .contains("postgres://postgres:postgres@postgres:5432/acme?sslmode=disable"),
        "pgweb should inherit the postgres database and password:\n{}",
        result.compose_yaml
    );
    assert!(
        pgweb.get("depends_on").is_some(),
        "pgweb should depend on postgres"
    );
    assert!(
        pgweb.get("healthcheck").is_some(),
        "pgweb should have a healthcheck"
    );
}

#[test]
fn dbgate_fragment_assembles_for_postgres() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![
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
                p.insert(
                    "connection_label".to_string(),
                    toml::Value::String("Acme dev".to_string()),
                );
                p
            },
            variant: None,
            config: None,
        },
    ];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let dbgate = validate_service(&doc, "dbgate");

    let image = dbgate.get("image").unwrap().as_str().unwrap();
    assert_eq!(
        image, "dbgate/dbgate:7.2.3",
        "dbgate image should be pinned to a concrete version tag"
    );
    assert!(
        dbgate.get("depends_on").is_some(),
        "dbgate should depend on postgres when referenced"
    );
    assert!(
        dbgate.get("healthcheck").is_some(),
        "dbgate should have a healthcheck"
    );
    let env = dbgate
        .get("environment")
        .expect("dbgate service declares environment block");
    assert_eq!(
        env.get("ENGINE_pg").unwrap().as_str().unwrap(),
        "postgres@dbgate-plugin-postgres"
    );
    assert_eq!(env.get("SERVER_pg").unwrap().as_str().unwrap(), "postgres");
    assert_eq!(env.get("DATABASE_pg").unwrap().as_str().unwrap(), "acme");
    assert_eq!(
        env.get("PASSWORD_pg").unwrap().as_str().unwrap(),
        "postgres"
    );
    assert_eq!(env.get("LABEL_pg").unwrap().as_str().unwrap(), "Acme dev");
    assert!(
        env.get("LOGIN").is_none() && env.get("PASSWORD").is_none(),
        "dbgate should not emit LOGIN/PASSWORD unless the `login` param is set"
    );

    // dbgate persists settings via the named `data` volume.
    let vol_names: std::collections::BTreeSet<&str> =
        result.volumes.iter().map(|v| v.name.as_str()).collect();
    assert!(
        vol_names.contains("test-dbgate-data"),
        "missing dbgate data volume; got {vol_names:?}"
    );
}

#[test]
fn dbgate_fragment_emits_login_env_when_credentials_are_set() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "dbgate".to_string(),
        catalog: "dbgate".to_string(),
        params: {
            let mut p = HashMap::new();
            p.insert("login".to_string(), toml::Value::String("dev".to_string()));
            p.insert(
                "password".to_string(),
                toml::Value::String("from-the-vault".to_string()),
            );
            p
        },
        variant: None,
        config: None,
    }];

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let dbgate = validate_service(&doc, "dbgate");
    let env = dbgate
        .get("environment")
        .expect("dbgate service declares environment block");

    assert_eq!(env.get("LOGIN").unwrap().as_str().unwrap(), "dev");
    assert_eq!(
        env.get("PASSWORD").unwrap().as_str().unwrap(),
        "from-the-vault"
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

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);
    let storage = validate_service(&doc, "storage");

    let image = storage.get("image").unwrap().as_str().unwrap();
    assert_eq!(
        image, "minio/minio:RELEASE.2025-09-07T16-13-09Z",
        "minio image should be pinned to a concrete version tag"
    );
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

    let result = assembler
        .assemble(&services, "test", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
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

    let result = assembler
        .assemble(&services, "full-stack", ".", ".effigy-catalog", 1000, 1000)
        .unwrap();
    let doc = validate_compose_structure(&result.compose_yaml);

    // All 8 services should be present.
    for name in &[
        "app", "web", "db", "cache", "sessions", "mail", "storage", "search",
    ] {
        validate_service(&doc, name);
    }

    // Should have named volumes for database, cache/runtime stores, storage, and search.
    assert_eq!(result.volumes.len(), 5);
    let vol_names: Vec<&str> = result.volumes.iter().map(|v| v.name.as_str()).collect();
    assert!(vol_names.iter().any(|n| n.contains("db-data")));
    assert!(vol_names.iter().any(|n| n.contains("pnpm-store")));
    assert!(vol_names.iter().any(|n| n.contains("cache-data")));
    assert!(vol_names.iter().any(|n| n.contains("storage")));
    assert!(vol_names.iter().any(|n| n.contains("search")));
    assert!(
        result
            .compose_yaml
            .contains("full-stack-db-data:/var/lib/mysql"),
        "should have a named mariadb data volume:\n{}",
        result.compose_yaml
    );
}
