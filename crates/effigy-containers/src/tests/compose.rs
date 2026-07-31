use super::*;

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

#[test]
fn load_container_policy_generates_compose_from_catalog_services() {
    with_temp_effigy_home("catalog-services", |_| {
        let root = temp_repo("catalog-services");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "clientname.test", tls = true, port = 8080 }]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");

        assert_eq!(policy.primary_service, "app");
        assert_eq!(policy.compose_source, EffectiveComposeSource::Generated);
        assert_eq!(policy.dns_domain.as_deref(), Some("clientname.test"));
        assert!(policy.dns_tls);
        assert_eq!(policy.dns_port, Some(8080));
        assert_eq!(policy.compose_files.len(), 2);
        assert!(
            policy
                .compose_file_display
                .contains(".effigy-compose.generated.yml"),
            "display should reference generated compose, got {}",
            policy.compose_file_display
        );

        let compose_path = root.join(".effigy/runtime/compose/.effigy-compose.generated.yml");
        assert_eq!(policy.compose_files[0], compose_path);
        assert!(compose_path.exists(), "generated compose file should exist");
        let dns_override = fs::read_to_string(&policy.compose_files[1]).expect("read dns override");
        assert!(
            dns_override.contains("app:\n    dns:"),
            "dns override should target app service: {dns_override}"
        );
        assert!(
            dns_override.contains("web:\n    dns:"),
            "dns override should target web service: {dns_override}"
        );
        assert!(
            dns_override.contains("extra_hosts:"),
            "dns override should include route parity hosts: {dns_override}"
        );
        assert!(
            dns_override.contains("\"clientname.test:192.168.5.2\""),
            "dns override should route project hostnames to the Colima gateway: {dns_override}"
        );

        let compose = fs::read_to_string(compose_path).expect("read generated compose");
        assert!(compose.contains("services:"));
        assert!(compose.contains("app:"));
        assert!(compose.contains("web:"));
        assert_eq!(
            policy.secret_delivery,
            effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv
        );
        assert!(!policy.ports_declared_explicitly);
        let http_port = policy
            .declared_ports
            .iter()
            .filter_map(|value| value.split_once(':'))
            .find(|(_, container)| *container == "80")
            .map(|(host, _)| host.to_owned())
            .expect("generated compose should expose a host port for container port 80");
        assert!(
            compose.contains(&format!("127.0.0.1:{http_port}:80")),
            "generated compose should bind published ports to loopback: {compose}"
        );
    });
}

#[test]
fn load_container_policy_preserves_secret_runtime_file_delivery() {
    with_temp_effigy_home("catalog-secret-runtime-files", |_| {
        let root = temp_repo("catalog-secret-runtime-files");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.secrets]
delivery = "runtime-files"
runtime_dir = "/run/effigy/secrets"
source_for_deferrals = true

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");

        assert_eq!(
            policy.secret_delivery,
            effigy_manifest::ManifestContainerSecretDelivery::RuntimeFiles
        );
        assert_eq!(
            policy.secret_runtime_dir.as_deref(),
            Some("/run/effigy/secrets")
        );
        assert!(policy.source_secret_runtime_for_deferrals);
    });
}

#[test]
fn generated_compose_workspace_app_shape_keeps_runtime_paths_and_external_mounts_stable() {
    with_temp_effigy_home("workspace-app-generated-compose-paths", |_| {
        let parent = tempfile::tempdir().expect("workspace app parent tempdir");
        let root = parent.path().join("workspace-app-reference");
        let platform = parent.path().join("platform");
        fs::create_dir_all(&root).expect("mkdir root");
        fs::create_dir_all(&platform).expect("mkdir workspace app sibling");
        let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../effigy-manifest/tests/fixtures/workspace-app-bundle");
        let bundle_dir = root.join("bundles/workspace-app");
        copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
[bundle]
base = {{ type = "path", dir = "bundles/workspace-app" }}
host = "workspace-app.test"
databases = ["workspace-app"]
project_name = "workspace-app-reference-dev"
workspace_subdir = "workspace-app-reference"

[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/workspace-root/workspace-app-reference"

[containers.web.host]
mounts = [
  {{ host = "{}",
     container = "/workspace-root/platform",
     external = true }},
]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
                platform.display()
            ),
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let compose = fs::read_to_string(&policy.compose_files[0]).expect("read compose");
        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_platform = platform.canonicalize().expect("canonical platform");
        let runtime_compose_dir = root.join(".effigy/runtime/compose");

        assert_eq!(policy.compose_source, EffectiveComposeSource::Generated);
        assert_eq!(policy.project_name, "workspace-app-reference-web-dev");
        assert!(policy.compose_files[0].starts_with(&runtime_compose_dir));
        assert!(
            policy.compose_files[0].exists(),
            "runtime compose path should exist"
        );
        assert!(
            policy
                .compose_file_display
                .contains(".effigy/runtime/compose/"),
            "display should stay under runtime compose dir: {}",
            policy.compose_file_display
        );
        assert!(
            compose.contains(&format!(
                "{}:/workspace-root/workspace-app-reference",
                canonical_root.display()
            )),
            "generated compose should mount the target repo at the workspace app workspace path: {compose}"
        );
        assert!(
            compose.contains(&format!(
                "{}:/workspace-root/platform",
                canonical_platform.display()
            )),
            "generated compose should preserve the external workspace app sibling mount: {compose}"
        );
    });
}

#[test]
fn generated_compose_uses_explicit_host_ports_when_declared() {
    with_temp_effigy_home("catalog-explicit-host-ports", |_| {
        let root = temp_repo("catalog-explicit-host-ports");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
ports = ["18080:80", "13306:3306"]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let compose =
            fs::read_to_string(root.join(".effigy/runtime/compose/.effigy-compose.generated.yml"))
                .expect("compose");

        assert!(compose.contains("127.0.0.1:18080:80"), "{compose}");
        assert!(compose.contains("127.0.0.1:13306:3306"), "{compose}");
        assert!(policy.ports_declared_explicitly);
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value == "18080:80"));
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value == "13306:3306"));
    });
}

#[test]
fn generated_compose_publish_address_can_opt_back_into_public_binding() {
    with_temp_effigy_home("catalog-public-publish-address", |_| {
        let root = temp_repo("catalog-public-publish-address");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
ports = ["18080:80"]
publish_address = "0.0.0.0"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let compose =
            fs::read_to_string(root.join(".effigy/runtime/compose/.effigy-compose.generated.yml"))
                .expect("compose");

        assert!(compose.contains("0.0.0.0:18080:80"), "{compose}");
        assert!(
            !compose.contains("127.0.0.1:18080:80"),
            "public publish address should replace the loopback default: {compose}"
        );
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value == "18080:80"));
    });
}

#[test]
fn generated_compose_rejects_invalid_publish_address() {
    with_temp_effigy_home("catalog-invalid-publish-address", |_| {
        let root = temp_repo("catalog-invalid-publish-address");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
publish_address = "not-an-ip"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
"#,
        )
        .expect("write manifest");

        let error = load_container_policy(&root, None).expect_err("should fail");
        assert!(
            error.to_string().contains("host.publish_address"),
            "unexpected error: {error}"
        );
    });
}

#[test]
fn direct_compose_prefers_manifest_host_ports_over_inferred_ports() {
    let root = temp_repo("direct-compose-explicit-ports");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[containers.web.host]
ports = ["18080:80"]
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  workspace:
    image: alpine
    ports:
      - "8080:80"
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert!(policy.ports_declared_explicitly);
    assert_eq!(policy.declared_ports, vec!["18080:80".to_owned()]);
}

#[test]
fn generated_compose_auto_allocates_stable_project_ports() {
    with_temp_effigy_home("catalog-auto-ports", |_| {
        let root = temp_repo("catalog-auto-ports");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let first = load_container_policy(&root, None).expect("first policy");
        let second = load_container_policy(&root, None).expect("second policy");
        assert!(!first.ports_declared_explicitly);
        assert_eq!(first.declared_ports, second.declared_ports);
        let http_port = first
            .declared_ports
            .iter()
            .filter_map(|value| value.split_once(':'))
            .find(|(_, container)| *container == "80")
            .map(|(host, _)| host.parse::<u16>().expect("host port"))
            .expect("expected generated compose HTTP port");
        let mysql_port = first
            .declared_ports
            .iter()
            .filter_map(|value| value.split_once(':'))
            .find(|(host, container)| *container == "3306" && *host != "3306")
            .map(|(host, _)| host.parse::<u16>().expect("host port"))
            .expect("expected generated compose MySQL port");
        assert_eq!(mysql_port, http_port + 6);
    });
}

#[test]
fn generated_compose_auto_allocates_distinct_ports_for_multiple_http_services() {
    with_temp_effigy_home("catalog-auto-ports-multi-http", |_| {
        let root = temp_repo("catalog-auto-ports-multi-http");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.dbadmin]
catalog = "phpmyadmin"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let compose =
            fs::read_to_string(root.join(".effigy/runtime/compose/.effigy-compose.generated.yml"))
                .expect("read generated compose");

        let http_ports = policy
            .declared_ports
            .iter()
            .filter_map(|value| value.split_once(':'))
            .filter(|(_, container)| *container == "80")
            .map(|(host, _)| host.parse::<u16>().expect("host port"))
            .collect::<Vec<_>>();

        assert_eq!(http_ports.len(), 2, "expected two HTTP services: {compose}");
        assert_ne!(
            http_ports[0], http_ports[1],
            "HTTP services should not share a host port:\n{compose}"
        );
        for port in &http_ports {
            assert!(
                compose.contains(&format!("- 127.0.0.1:{port}:80")),
                "generated compose should include HTTP port {port} bound to loopback:\n{compose}"
            );
        }
    });
}

#[test]
fn generated_compose_keeps_runtime_ports_for_tcp_alias_services() {
    with_temp_effigy_home("catalog-loopback-tcp-aliases", |home| {
        let root = temp_repo("catalog-loopback-tcp-aliases");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
host_ports = ["41001:41001"]

[containers.stack.services.postgres]
catalog = "postgres"
database = "acme"
password = "postgres"

[containers.stack.services.mailpit]
catalog = "mailpit"

[containers.stack.services.minio]
catalog = "minio"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let compose =
            fs::read_to_string(root.join(".effigy/runtime/compose/.effigy-compose.generated.yml"))
                .expect("compose");

        assert!(!compose.contains("127.1.0.1:5432:5432"), "{compose}");
        assert!(!compose.contains("127.1.0.1:1025:1025"), "{compose}");
        assert!(!compose.contains("127.1.0.1:9000:9000"), "{compose}");
        assert!(compose.contains(":8025"), "{compose}");
        assert!(compose.contains(":9001"), "{compose}");
        for container_port in ["5432", "1025", "9000"] {
            let host_port = policy
                .declared_ports
                .iter()
                .filter_map(|value| value.split_once(':'))
                .find(|(_, container)| *container == container_port)
                .map(|(host, _)| host.to_owned())
                .unwrap_or_else(|| panic!("expected declared port for {container_port}"));
            assert!(
                compose.contains(&format!("127.0.0.1:{host_port}:{container_port}")),
                "tcp-alias service port {container_port} should stay published on loopback:\n{compose}"
            );
        }
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value.ends_with(":5432")));
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value.ends_with(":1025")));
        assert!(policy
            .declared_ports
            .iter()
            .any(|value| value.ends_with(":9000")));
        assert!(home.join("gateway").join("loopback-ips.json").exists());
    });
}

#[test]
fn generated_php_workspace_defaults_to_non_root_identity() {
    with_temp_effigy_home("catalog-php-workspace-user", |_| {
        let root = temp_repo("catalog-php-workspace-user");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("policy");
        assert_eq!(policy.workspace_user.as_deref(), Some("dev"));
        assert_eq!(policy.workspace_home.as_deref(), Some("/home/dev"));
    });
}

#[test]
fn generated_services_infer_working_dir_from_container_config() {
    with_temp_effigy_home("catalog-container-working-dir", |_| {
        let root = temp_repo("catalog-container-working-dir");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"
working_dir = "/var/www/html"

[containers.web.services.app]
catalog = "php-fpm"
document_root = "."
version = "8.4"

[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = "/vendor/genesis.php"
error_page_404 = "/vendor/genesis.php"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("policy");
        let compose = fs::read_to_string(&policy.compose_files[0]).expect("read compose");

        assert!(
            compose.contains("working_dir: /var/www/html")
                || compose.contains("working_dir: \"/var/www/html\""),
            "generated compose should include the container working_dir: {compose}"
        );
        assert!(
            compose.contains(&format!("{}:/var/www/html", root.display())),
            "generated compose should mount the repo at the container working_dir: {compose}"
        );
    });
}

#[test]
fn generated_php_workspace_mounts_host_composer_home_when_enabled() {
    with_temp_effigy_home("catalog-php-composer-home", |_| {
        let root = temp_repo("catalog-php-composer-home");
        let host_composer_home = root.join("host-composer-home");
        fs::create_dir_all(&host_composer_home).expect("mkdir host composer home");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
mount_host_composer_home = true

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
        )
        .expect("write manifest");

        let manifest = load_task_manifest(&root.join("effigy.toml")).expect("load manifest");
        let composer_mount_flag = manifest
            .containers
            .as_ref()
            .and_then(|containers| containers.environments.get("web"))
            .and_then(|container| container.services.get("app"))
            .and_then(|service| service.params.get("mount_host_composer_home"))
            .and_then(|value| value.as_bool());
        assert_eq!(composer_mount_flag, Some(true));

        let policy = with_test_host_composer_home(Some(&host_composer_home), || {
            load_container_policy(&root, Some("web")).expect("policy")
        });
        let rewritten = fs::read_to_string(&policy.compose_files[0]).expect("read compose");

        assert!(
            rewritten.contains(&format!(
                "{}:/home/dev/.config/composer",
                host_composer_home.display()
            )),
            "rewritten compose should mount the host composer home: {rewritten}"
        );
    });
}

#[test]
fn generated_php_workspace_does_not_mount_host_composer_home_by_default() {
    with_temp_effigy_home("catalog-php-composer-home-default-off", |_| {
        let root = temp_repo("catalog-php-composer-home-default-off");
        let host_composer_home = root.join("host-composer-home");
        fs::create_dir_all(&host_composer_home).expect("mkdir host composer home");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
        )
        .expect("write manifest");

        let policy = with_test_host_composer_home(Some(&host_composer_home), || {
            load_container_policy(&root, Some("web")).expect("policy")
        });
        let rewritten = fs::read_to_string(&policy.compose_files[0]).expect("read compose");

        assert!(
            !rewritten.contains(&format!(
                "{}:/home/dev/.config/composer",
                host_composer_home.display()
            )),
            "rewritten compose should not mount the host composer home by default: {rewritten}"
        );
        assert!(
            rewritten.contains(":/home/dev/.config/composer"),
            "rewritten compose should mount a shared composer home by default: {rewritten}"
        );
        assert!(
            rewritten.contains("/shared/composer-cache:/home/dev/.cache/composer"),
            "rewritten compose should mount shared composer cache by default: {rewritten}"
        );
    });
}

#[test]
fn generated_php_workspace_can_disable_shared_composer_state_mounts() {
    with_temp_effigy_home("catalog-php-composer-shared-state-opt-out", |_| {
        let root = temp_repo("catalog-php-composer-shared-state-opt-out");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
mount_shared_composer_auth = false
mount_shared_composer_cache = false

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, Some("web")).expect("policy");
        let rewritten = fs::read_to_string(&policy.compose_files[0]).expect("read compose");

        assert!(
            !rewritten.contains(":/home/dev/.config/composer"),
            "rewritten compose should not mount shared composer home when disabled: {rewritten}"
        );
        assert!(
            !rewritten.contains("/shared/composer-cache:/home/dev/.cache/composer"),
            "rewritten compose should not mount shared composer cache when disabled: {rewritten}"
        );
    });
}

#[test]
fn generated_php_workspace_host_integration_and_shared_service_stack_proof_stays_stable() {
    with_temp_effigy_home("catalog-php-host-integration-proof", |_| {
        let root = temp_repo("catalog-php-host-integration-proof");
        let host_composer_home = root.join("host-composer-home");
        let ssh_dir = root.join("ssh-home");
        let sibling = tempfile::tempdir().expect("external sibling tempdir");
        fs::create_dir_all(&host_composer_home).expect("mkdir host composer home");
        fs::create_dir_all(&ssh_dir).expect("mkdir ssh dir");
        fs::create_dir_all(sibling.path().join("public")).expect("mkdir sibling public");
        fs::write(ssh_dir.join("config"), "Host gideonreeling.co.uk\n").expect("write ssh config");
        fs::write(
            root.join("effigy.toml"),
            format!(
                r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.host]
mounts = [
  {{ host = "{}",
     container = "/var/www/mortcalc",
     external = true }},
]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
mount_host_composer_home = true
ssh_dir_path = "{}"

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
                sibling.path().display(),
                ssh_dir.display()
            ),
        )
        .expect("write manifest");

        let policy = with_test_host_composer_home(Some(&host_composer_home), || {
            load_container_policy(&root, Some("web")).expect("policy")
        });
        let rewritten = fs::read_to_string(&policy.compose_files[0]).expect("read compose");

        assert!(
            rewritten.contains(&format!(
                "{}:/home/dev/.config/composer",
                host_composer_home.display()
            )),
            "rewritten compose should mount the host composer home: {rewritten}"
        );
        assert!(
            !rewritten.contains("effigy-shared-composer-home:/home/dev/.config/composer"),
            "workspace proof should not fall back to the shared composer-home volume when host composer home is enabled: {rewritten}"
        );
        assert!(
            rewritten.contains(&format!("{}:/home/dev/.ssh:ro", ssh_dir.display())),
            "rewritten compose should mount the explicit host ssh dir read-only: {rewritten}"
        );
        assert!(
            rewritten.contains(&format!(
                "{}:/var/www/mortcalc",
                sibling.path().canonicalize().unwrap().display()
            )),
            "rewritten compose should preserve the external host mount: {rewritten}"
        );

        assert_eq!(policy.shared_services.len(), 1);
        let shared = &policy.shared_services[0];
        assert_eq!(
            shared.standard_env_vars(),
            vec![
                ("DB_HOST".to_owned(), "host.docker.internal".to_owned()),
                ("MYSQL_HOST".to_owned(), "host.docker.internal".to_owned()),
                ("DB_PORT".to_owned(), shared.host_port.to_string()),
                ("MYSQL_PORT".to_owned(), shared.host_port.to_string()),
            ]
        );
        assert!(rewritten.contains("DB_HOST: host.docker.internal"));
        assert!(rewritten.contains("MYSQL_HOST: host.docker.internal"));
        assert!(rewritten.contains(&format!("DB_PORT: '{}'", shared.host_port)));
        assert!(rewritten.contains(&format!("MYSQL_PORT: '{}'", shared.host_port)));
    });
}

#[test]
fn generated_compose_rewrites_shared_backing_services() {
    with_temp_effigy_home("catalog-shared-db", |home| {
        let root = temp_repo("catalog-shared-db");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        assert_eq!(policy.shared_services.len(), 1);
        let shared = &policy.shared_services[0];
        assert_eq!(shared.service_name, "db");
        assert_eq!(shared.catalog, "mariadb");
        assert_eq!(shared.container_port, 3306);

        let compose =
            fs::read_to_string(root.join(".effigy/runtime/compose/.effigy-compose.generated.yml"))
                .expect("compose");
        assert!(!compose.contains("\n  db:\n"));
        assert!(compose.contains("DB_HOST: host.docker.internal"));
        assert!(compose.contains(&format!("DB_PORT: '{}'", shared.host_port)));

        assert!(shared.compose_file.exists());
        let shared_compose = fs::read_to_string(&shared.compose_file).expect("read shared compose");
        assert!(
            shared_compose.contains(&format!("127.0.0.1:{}:3306", shared.host_port)),
            "shared compose should bind the runtime host port to loopback: {shared_compose}"
        );
        assert!(home
            .join("shared-services")
            .join(&shared.project_name)
            .join(".effigy-compose.generated.yml")
            .exists());
    });
}

#[test]
fn generated_compose_rejects_unsupported_shared_catalog() {
    with_temp_effigy_home("catalog-shared-unsupported", |_| {
        let root = temp_repo("catalog-shared-unsupported");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
shared = true
"#,
        )
        .expect("write manifest");

        let error = load_container_policy(&root, None).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("unsupported shared catalog `php-fpm`"));
    });
}

#[test]
fn shared_service_compose_keeps_runtime_host_port_and_adds_loopback_binding() {
    with_temp_effigy_home("catalog-shared-loopback-binding", |home| {
        let root = temp_repo("catalog-shared-loopback-binding");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.db]
catalog = "mariadb"
shared = true
version = "10.11"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");

        let policy = load_container_policy(&root, None).expect("policy");
        let shared = &policy.shared_services[0];
        let shared_compose = fs::read_to_string(&shared.compose_file).expect("read shared compose");

        assert!(
            !shared_compose.contains("127.1.0.1:3306:3306"),
            "{shared_compose}"
        );
        assert!(
            shared_compose.contains(&format!("127.0.0.1:{}:3306", shared.host_port)),
            "shared compose should bind the runtime host port to loopback: {shared_compose}"
        );
        assert!(home.join("gateway").join("loopback-ips.json").exists());
    });
}

#[test]
fn load_container_policy_rejects_compose_file_and_services_together() {
    let root = temp_repo("mixed-compose-source");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let error = load_container_policy(&root, None).expect_err("should fail");
    assert!(error
        .to_string()
        .contains("cannot combine `compose_file` with"));
}

#[test]
fn direct_compose_policy_writes_runtime_dns_override_for_declared_services() {
    let root = temp_repo("direct-compose-runtime-dns");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  app:
    image: "node:22"
  db:
    image: "postgres:16"
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.compose_files.len(), 2);
    let dns_override = fs::read_to_string(&policy.compose_files[1]).expect("read dns override");
    assert!(
        dns_override.contains("app:\n    dns:"),
        "dns override should target app service: {dns_override}"
    );
    assert!(
        dns_override.contains("db:\n    dns:"),
        "dns override should target db service: {dns_override}"
    );
    assert!(
        dns_override.contains("\"1.1.1.1\""),
        "dns override: {dns_override}"
    );
    assert!(
        dns_override.contains("\"8.8.8.8\""),
        "dns override: {dns_override}"
    );
    assert!(
        !dns_override.contains("extra_hosts:"),
        "dns override should not invent route parity hosts without domains: {dns_override}"
    );
}

#[test]
fn generated_compose_runtime_dns_override_includes_project_and_alias_domains() {
    let root = temp_repo("generated-compose-route-parity");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "workspace"

[containers.stack.dns]
routes = [
  { domain = "acme.test", tls = false, service = "workspace" },
  { domain = "api.acme.test", tls = false, service = "workspace", port = 41001 },
]

[containers.stack.services.workspace]
catalog = "workspace-rust-bun"
working_subdir = "generated-compose-route-parity"

[containers.stack.services.postgres]
catalog = "postgres"

[containers.stack.services.mailpit]
catalog = "mailpit"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.compose_files.len(), 2);
    let dns_override = fs::read_to_string(&policy.compose_files[1]).expect("read dns override");
    assert!(
        dns_override.contains("\"acme.test:192.168.5.2\""),
        "dns override should include base project domain: {dns_override}"
    );
    assert!(
        dns_override.contains("\"api.acme.test:192.168.5.2\""),
        "dns override should include explicit subdomain routes: {dns_override}"
    );
    assert!(
        dns_override.contains("postgres:\n    dns:")
            && dns_override.contains("\"postgres.acme.test\""),
        "dns override should include postgres network alias domains: {dns_override}"
    );
    assert!(
        dns_override.contains("mailpit:\n    dns:") && dns_override.contains("\"smtp.acme.test\""),
        "dns override should include mailpit network alias domains: {dns_override}"
    );
    assert!(
        !dns_override.contains("\"postgres.acme.test:192.168.5.2\""),
        "TCP service aliases must not point back at the host gateway: {dns_override}"
    );
    assert!(
        !dns_override.contains("\"smtp.acme.test:192.168.5.2\""),
        "TCP service aliases must not point back at the host gateway: {dns_override}"
    );
}

#[test]
fn direct_compose_policy_rewrites_workspace_mounts_from_manifest_contract() {
    let parent = std::env::temp_dir().join(format!(
        "effigy-direct-compose-workspace-mounts-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let root = parent.join("workspace-app-reference");
    let platform = parent.join("platform");
    let poodle = parent.join("poodle");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
    fs::create_dir_all(&platform).expect("mkdir workspace app");
    fs::create_dir_all(&poodle).expect("mkdir poodle");
    fs::write(
        poodle.join("effigy.toml"),
        r#"
[isolation]
paths = ["node_modules", ".svelte-kit"]
"#,
    )
    .expect("write poodle manifest");
    fs::write(
        root.join("infra/dev/workspace.Dockerfile"),
        "FROM node:22\n",
    )
    .expect("dockerfile");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[systems.dev]
container = "stack"
user = "dev"
home = "/home/dev"
working_dir = "/workspace-root/workspace-app-reference"
mounts = ["../platform", "../poodle"]

[containers.stack]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  workspace:
    build:
      context: ../..
      dockerfile: infra/dev/workspace.Dockerfile
    volumes:
      - ../../../:/workspace-root
      - stack-cache:/cache
volumes:
  stack-cache: {}
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let rewritten =
        fs::read_to_string(&policy.compose_files[0]).expect("read rewritten workspace compose");
    let expected_rewrite_path = root.join(".effigy/runtime/compose/stack.workspace.compose.yml");
    let canonical_root = root.canonicalize().expect("canonical repo root");

    assert_eq!(policy.compose_files[0], expected_rewrite_path);
    assert!(expected_rewrite_path.exists(), "rewrite path should exist");
    assert!(
        rewritten.contains(":/workspace-root/workspace-app-reference"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains(":/workspace-root/platform"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains(":/workspace-root/poodle"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains("efi-iso-")
            && rewritten.contains(":/workspace-root/poodle/node_modules"),
        "rewritten compose should overlay poodle node_modules isolation with a named volume: {rewritten}"
    );
    assert!(
        rewritten.contains("efi-iso-")
            && rewritten.contains(":/workspace-root/poodle/.svelte-kit"),
        "rewritten compose should overlay poodle .svelte-kit isolation with a named volume: {rewritten}"
    );
    assert!(
        !rewritten.contains("../../../:/workspace-root"),
        "rewritten compose should remove broad parent mount: {rewritten}"
    );
    assert!(
        rewritten.contains(":/cache") && rewritten.contains("name: efv-"),
        "rewritten compose should preserve and compact named volumes: {rewritten}"
    );
    assert!(
        rewritten.contains("name: efi-iso-"),
        "rewritten compose should declare poodle node_modules isolation volume: {rewritten}"
    );
    assert!(
        !rewritten.contains("user: dev"),
        "rewritten compose should not force runtime user: {rewritten}"
    );
    assert!(
        !rewritten.contains("HOME: /home/dev"),
        "rewritten compose should not force runtime HOME: {rewritten}"
    );
    assert!(
        rewritten.contains(&format!("context: {}", canonical_root.display())),
        "rewritten compose should preserve relocated build context: {rewritten}"
    );
    assert!(
        rewritten.contains("dockerfile: infra/dev/workspace.Dockerfile"),
        "rewritten compose should preserve dockerfile path relative to build context: {rewritten}"
    );
    assert_eq!(policy.workspace_user.as_deref(), Some("dev"));
    assert_eq!(policy.workspace_home.as_deref(), Some("/home/dev"));
}

#[test]
fn generated_php_nginx_stack_compacts_mirrored_named_volumes_across_services() {
    let root = temp_repo("catalog-php-nginx-shared-volume-compact");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
document_root = "."
isolated_dirs = ["vendor", "node_modules"]

[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = "/vendor/genesis.php"
error_page_404 = "/vendor/genesis.php"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/var/www/html"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, Some("web")).expect("policy");
    let rewritten =
        fs::read_to_string(&policy.compose_files[0]).expect("read rewritten workspace compose");

    assert!(
        rewritten.contains("efv-"),
        "rewritten compose should compact named volume names: {rewritten}"
    );
    assert!(
        !rewritten.contains("catalog-php-nginx-shared-volume-compact-web-app-var-www-html-vendor"),
        "rewritten compose should not leave the long vendor volume name behind: {rewritten}"
    );
    assert!(
        !rewritten
            .contains("catalog-php-nginx-shared-volume-compact-web-app-var-www-html-node-modules"),
        "rewritten compose should not leave the long node_modules volume name behind: {rewritten}"
    );
    assert!(
        rewritten.contains(":/var/www/html/vendor:ro"),
        "rewritten compose should keep the nginx read-only vendor mirror mount: {rewritten}"
    );
    assert!(
        rewritten.contains(":/var/www/html/node_modules:ro"),
        "rewritten compose should keep the nginx read-only node_modules mirror mount: {rewritten}"
    );
}
