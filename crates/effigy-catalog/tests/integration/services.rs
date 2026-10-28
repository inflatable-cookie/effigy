use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
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

    // Any HTTP status line means nginx is serving; BusyBox wget (nginx:alpine)
    // exits 1 on a 404 just as on a refused connection, so the exit code alone
    // can't tell them apart.
    assert!(
        command.contains("wget -S --spider http://127.0.0.1:80/")
            && command.contains("grep -q 'HTTP/'"),
        "nginx healthcheck should treat any HTTP response as ready, got: {command}"
    );
    let (interpolated, warnings) = compose_interpolate(command);
    assert!(
        warnings.is_empty(),
        "health command should not trigger Compose substitution warnings, got {warnings:?} from {command}"
    );
    assert_eq!(
        nginx_health_status_for_wget(&interpolated, "  HTTP/1.1 200 OK", 0),
        0,
        "HTTP success should be healthy"
    );
    assert_eq!(
        nginx_health_status_for_wget(&interpolated, "  HTTP/1.1 404 Not Found", 1),
        0,
        "an HTTP 404 from BusyBox wget (exit 1) should still be healthy"
    );
    assert_eq!(
        nginx_health_status_for_wget(
            &interpolated,
            "wget: can't connect to remote host (127.0.0.1): Connection refused",
            1
        ),
        1,
        "a refused connection should be unhealthy"
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

fn nginx_health_status_for_wget(command: &str, stderr_line: &str, wget_exit: i32) -> i32 {
    let dir = tempfile::tempdir().unwrap();
    let wget = dir.path().join("wget");
    std::fs::write(
        &wget,
        format!("#!/bin/sh\necho '{stderr_line}' >&2\nexit {wget_exit}\n"),
    )
    .unwrap();
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

    // The image ships no wget or curl, so the probe must use PHP and survive
    // Compose interpolation with its PHP variables intact.
    let probe = admin["healthcheck"]["test"]
        .as_sequence()
        .expect("healthcheck test")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        probe.first().copied(),
        Some("CMD-SHELL"),
        "phpMyAdmin's check runs on nerdctl, which joins a CMD exec form without quoting: {probe:?}"
    );
    let command = probe.get(1).copied().expect("healthcheck command");
    assert!(
        command.starts_with("php -r '") && command.ends_with(");'"),
        "phpMyAdmin's PHP code should be one shell-quoted argument, got: {command}"
    );
    let (interpolated, warnings) = compose_interpolate(command);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        interpolated.contains("fsockopen(\"127.0.0.1\", 80, $e, $s, 2)"),
        "{interpolated}"
    );

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

// --- Catalog health checks use tools their images ship -------------------

/// The executable names each embedded catalog image is known to ship.
///
/// Every entry was confirmed against the pinned image with a throwaway
/// `nerdctl run --rm --network none --entrypoint …` on 2026-10-24, or against
/// the image's published Dockerfile where the tag can no longer be pulled:
/// `workspace-rust-bun` is built from the Dockerfile in this catalog. Only the
/// tools a health check may name are listed, so a fragment that starts naming
/// a new one has to add the evidence here first, instead of depending on a
/// binary its image lacks (effigy#454, #323).
const IMAGE_TOOLS: &[(&str, &[&str])] = &[
    // node:bookworm-slim base: no wget, no curl.
    ("dbgate", &["node"]),
    // Ubuntu base with curl.
    ("elasticsearch", &["curl"]),
    // Alpine base with BusyBox wget.
    ("mailpit", &["wget"]),
    ("mariadb", &["healthcheck.sh"]),
    // Alpine base with BusyBox wget and grep.
    ("nginx", &["wget", "grep"]),
    // Debian base with curl; no wget.
    ("pgweb", &["curl"]),
    ("phpmyadmin", &["php"]),
    ("postgres", &["pg_isready"]),
    // PGSTY Silo, a maintained S3-compatible server, ships the `mc` client.
    ("silo", &["mc"]),
    // Built from this catalog's Dockerfile: Rust toolchain and bun.
    ("workspace-rust-bun", &["cargo", "bun"]),
];

/// Every embedded fragment's health check may only name a tool its image
/// ships. A new fragment must declare its image's tools in [`IMAGE_TOOLS`].
#[test]
fn every_catalog_healthcheck_names_only_tools_its_image_ships() {
    let mut checked: Vec<String> = Vec::new();
    for (catalog, healthcheck) in catalog_healthchecks() {
        let test = healthcheck
            .get("test")
            .unwrap_or_else(|| panic!("{catalog} declares a healthcheck with no `test`"));
        let binaries = healthcheck_binaries(test);
        assert!(
            !binaries.is_empty(),
            "{catalog} health check names no program the test can see: {test:?}"
        );
        let tools = IMAGE_TOOLS
            .iter()
            .find(|(name, _)| *name == catalog)
            .map(|(_, tools)| *tools)
            .unwrap_or_else(|| {
                panic!(
                    "{catalog} declares a healthcheck but IMAGE_TOOLS has no entry for its \
                     image. Confirm which binaries the pinned image ships and add them"
                )
            });
        for binary in &binaries {
            assert!(
                tools.contains(&binary.as_str()),
                "{catalog} health check names `{binary}`, which its image is not known to \
                 ship (known: {tools:?}). Use a tool the image ships, or add the evidence for \
                 `{binary}` to IMAGE_TOOLS"
            );
        }
        checked.push(catalog);
    }
    for (catalog, _) in IMAGE_TOOLS {
        assert!(
            checked.iter().any(|name| name == catalog),
            "IMAGE_TOOLS lists `{catalog}`, but its fragment declares no healthcheck"
        );
    }
}

/// A check is exact on every backend only when the fragment writes shell form
/// itself, or keeps every `CMD` exec-form argument a plain shell word.
///
/// `nerdctl compose` does not support the exec form: it joins the argv with
/// spaces into one unquoted `/bin/sh -c` command (effigy#324). An argument
/// holding a space or a shell metacharacter then changes the command — a
/// `node -e` or `php -r` probe fails to parse on every interval — so such a
/// fragment writes `CMD-SHELL` with its own quoting instead.
#[test]
fn every_catalog_healthcheck_is_exact_on_nerdctl() {
    let mut shell_form: Vec<String> = Vec::new();
    let mut exec_form: Vec<String> = Vec::new();
    for (catalog, healthcheck) in catalog_healthchecks() {
        let test = healthcheck
            .get("test")
            .unwrap_or_else(|| panic!("{catalog} declares a healthcheck with no `test`"));
        let Some(parts) = test.as_sequence() else {
            // A bare string is the shell form Compose already runs.
            shell_form.push(catalog);
            continue;
        };
        let words = parts
            .iter()
            .map(|part| {
                part.as_str().unwrap_or_else(|| {
                    panic!("{catalog} healthcheck entry is not a string: {part:?}")
                })
            })
            .collect::<Vec<_>>();
        match words.first().copied() {
            Some("CMD-SHELL") => shell_form.push(catalog),
            Some("NONE") => {}
            Some("CMD") => {
                for argument in &words[1..] {
                    assert!(
                        is_plain_shell_word(argument),
                        "{catalog} health check has the exec-form argument `{argument}`, which \
                         `nerdctl compose` would split or expand when it joins the exec form into \
                         an unquoted shell command. Write the check as `CMD-SHELL` with its own \
                         quoting (effigy#324)"
                    );
                }
                exec_form.push(catalog);
            }
            other => panic!("{catalog} has an unknown healthcheck form `{other:?}`: {test:?}"),
        }
    }
    assert!(
        shell_form.len() + exec_form.len() > 0,
        "no catalog health check was classified"
    );
}

/// A word a POSIX shell reads back unchanged when words are joined with
/// spaces: no quoting, expansion or list operator can apply to it.
fn is_plain_shell_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=@%+,".contains(character))
}

/// Every embedded fragment that declares a health check, with the catalog
/// name that fragment's directory uses.
fn catalog_healthchecks() -> Vec<(String, serde_yaml::Value)> {
    let catalog_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&catalog_root)
        .unwrap_or_else(|error| panic!("read {}: {error}", catalog_root.display()))
    {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let catalog = entry.file_name().into_string().unwrap();
        if let Some(healthcheck) = fragment_healthcheck(&catalog) {
            found.push((catalog, healthcheck));
        }
    }
    assert!(
        !found.is_empty(),
        "no catalog fragment healthcheck was found under {}",
        catalog_root.display()
    );
    found
}

/// Read the `healthcheck` block of one catalog fragment.
///
/// Fragments are templates, but a health check is plain Compose YAML: its
/// command is fixed, and those bytes are what a person reviews. Reading the
/// block directly judges the fragment before any parameter fills it in.
fn fragment_healthcheck(catalog: &str) -> Option<serde_yaml::Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("catalog")
        .join(catalog)
        .join("compose.fragment.yml");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut block = String::from("healthcheck:\n");
    let mut found = false;
    for line in source.lines() {
        if !found {
            if line.trim_end() == "    healthcheck:" {
                found = true;
            }
            continue;
        }
        if line.trim().is_empty() {
            block.push('\n');
            continue;
        }
        // Service keys sit at four spaces; the block ends when one starts.
        if !line.starts_with("     ") {
            break;
        }
        block.push_str(&line[4..]);
        block.push('\n');
    }
    if !found {
        return None;
    }
    let parsed: serde_yaml::Value = serde_yaml::from_str(&block).unwrap_or_else(|error| {
        panic!(
            "{} healthcheck block is not YAML: {error}\n{block}",
            path.display()
        )
    });
    parsed.get("healthcheck").cloned()
}

/// The executable names a Compose health check invokes.
fn healthcheck_binaries(test: &serde_yaml::Value) -> Vec<String> {
    match test {
        serde_yaml::Value::Sequence(parts) => {
            let words: Vec<&str> = parts
                .iter()
                .map(|part| {
                    part.as_str().unwrap_or_else(|| {
                        panic!("healthcheck test entry is not a string: {part:?}")
                    })
                })
                .collect();
            match words.first().copied() {
                Some("CMD") => words
                    .get(1)
                    .map(|binary| {
                        let binary = *binary;
                        vec![binary.rsplit('/').next().unwrap_or(binary).to_string()]
                    })
                    .unwrap_or_default(),
                Some("CMD-SHELL") => {
                    let command = words
                        .get(1)
                        .unwrap_or_else(|| panic!("CMD-SHELL without a command: {test:?}"));
                    shell_command_binaries(command)
                }
                Some(other) => {
                    panic!(
                        "unknown healthcheck form `{other}`, expected CMD or CMD-SHELL: {test:?}"
                    )
                }
                None => Vec::new(),
            }
        }
        serde_yaml::Value::String(command) => shell_command_binaries(command),
        other => panic!("healthcheck test is neither a sequence nor a string: {other:?}"),
    }
}

#[test]
fn a_healthcheck_test_is_read_into_the_programs_it_names() {
    let shell: serde_yaml::Value = serde_yaml::from_str(
        r#"["CMD-SHELL", "wget -qO- http://127.0.0.1:3000/ >/dev/null 2>&1 || exit 1"]"#,
    )
    .unwrap();
    assert_eq!(healthcheck_binaries(&shell), ["wget"]);

    let pipeline: serde_yaml::Value = serde_yaml::from_str(
        r#"["CMD-SHELL", "wget -S --spider http://127.0.0.1:80/ 2>&1 | grep -q 'HTTP/'"]"#,
    )
    .unwrap();
    assert_eq!(healthcheck_binaries(&pipeline), ["wget", "grep"]);

    let direct: serde_yaml::Value =
        serde_yaml::from_str(r#"["CMD", "node", "-e", "process.exit(0)"]"#).unwrap();
    assert_eq!(healthcheck_binaries(&direct), ["node"]);

    let probe: serde_yaml::Value = serde_yaml::from_str(
        r#"["CMD-SHELL", "command -v cargo >/dev/null && command -v bun >/dev/null"]"#,
    )
    .unwrap();
    assert_eq!(healthcheck_binaries(&probe), ["cargo", "bun"]);

    // An interpreter probe quotes its program text, so the `;`, `:` and
    // parentheses inside it are not further commands.
    let javascript: serde_yaml::Value = serde_yaml::from_str(
        r#"["CMD-SHELL", "node -e 'require(\"http\").get(\"http://127.0.0.1:3000/\",r=>{r.resume();process.exit(0)}).on(\"error\",e=>{console.error(e.message);process.exit(1)})'"]"#,
    )
    .unwrap();
    assert_eq!(healthcheck_binaries(&javascript), ["node"]);

    let php: serde_yaml::Value = serde_yaml::from_str(
        r#"["CMD-SHELL", "php -r 'exit(@fsockopen(\"127.0.0.1\", 80, $e, $s, 2) ? 0 : 1);'"]"#,
    )
    .unwrap();
    assert_eq!(healthcheck_binaries(&php), ["php"]);
}

/// The programs a shell health check runs: the command word of every pipeline
/// or list segment, where `command -v <name>` names `<name>` instead. Quoted
/// text stays inside its segment, so the JavaScript or PHP a probe passes to
/// an interpreter is not read as more commands.
fn shell_command_binaries(command: &str) -> Vec<String> {
    let mut programs = Vec::new();
    for segment in shell_segments(command) {
        let words: Vec<&str> = segment.split_whitespace().collect();
        let Some(first) = words.first().copied() else {
            continue;
        };
        let first = first.trim_start_matches('(');
        if first.is_empty() {
            continue;
        }
        // Shell builtins are not images' binaries.
        if matches!(
            first,
            "exit" | "true" | "false" | "test" | "[" | "return" | "echo" | ":"
        ) {
            continue;
        }
        if matches!(first, "command" | "exec" | "env") {
            if let Some(name) = words.iter().skip(1).find(|word| !word.starts_with('-')) {
                programs.push(name.trim_matches(['\'', '"']).to_string());
            }
            continue;
        }
        programs.push(
            first
                .trim_matches(['\'', '"'])
                .rsplit('/')
                .next()
                .unwrap_or(first)
                .to_string(),
        );
    }
    programs
}

/// Split a shell command where the shell starts a new list or pipeline
/// segment: an unquoted `|`, `||`, `&&`, `;` or newline. Quoted runs — and
/// backslash escapes — are kept, including a quote character inside the other
/// quote style.
fn shell_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut characters = command.chars().peekable();
    while let Some(character) = characters.next() {
        match quote {
            Some(open) if open == '"' && character == '\\' => {
                current.push(character);
                if let Some(escaped) = characters.next() {
                    current.push(escaped);
                }
            }
            Some(open) => {
                current.push(character);
                if character == open {
                    quote = None;
                }
            }
            None => match character {
                '\'' | '"' => {
                    quote = Some(character);
                    current.push(character);
                }
                '\\' => {
                    current.push(character);
                    if let Some(escaped) = characters.next() {
                        current.push(escaped);
                    }
                }
                '|' | ';' | '\n' => {
                    if matches!(character, '|') && characters.peek() == Some(&'|') {
                        characters.next();
                    }
                    segments.push(std::mem::take(&mut current));
                }
                '&' if characters.peek() == Some(&'&') => {
                    characters.next();
                    segments.push(std::mem::take(&mut current));
                }
                other => current.push(other),
            },
        }
    }
    segments.push(current);
    segments
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
fn silo_fragment_assembles_object_storage_with_volume() {
    let resolver = bundled_resolver();
    let assembler = ComposeAssembler::new(resolver);

    let services = vec![ServiceDeclaration {
        name: "storage".to_string(),
        catalog: "silo".to_string(),
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
    assert!(
        image.starts_with("docker.io/pgsty/silo:RELEASE.") && image.contains("@sha256:"),
        "silo image should stay digest-pinned: {image}"
    );

    let env = storage
        .get("environment")
        .expect("silo service declares environment block");
    assert_eq!(
        env.get("MINIO_ROOT_USER").unwrap().as_str().unwrap(),
        "minioadmin",
        "Silo keeps the MinIO-compatible root user name"
    );
    assert_eq!(
        env.get("MINIO_ROOT_PASSWORD").unwrap().as_str().unwrap(),
        "minioadmin",
        "Silo keeps the MinIO-compatible root password name"
    );

    let ports: Vec<&str> = storage
        .get("ports")
        .unwrap()
        .as_sequence()
        .unwrap()
        .iter()
        .filter_map(|port| port.as_str())
        .collect();
    assert_eq!(
        ports,
        vec!["9000:9000", "9001:9001"],
        "S3 API (9000) and console (9001) keep their ports"
    );

    let healthcheck = storage.get("healthcheck").expect("silo healthcheck");
    let health_test = healthcheck
        .get("test")
        .unwrap()
        .as_sequence()
        .unwrap()
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        health_test,
        vec!["CMD", "mc", "ready", "local"],
        "silo keeps the `mc ready local` health contract"
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
            catalog: "silo".to_string(),
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
