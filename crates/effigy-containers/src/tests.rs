use super::{
    data_list_report, data_transfer_report, effective_attach_mode, eject_generated_compose,
    load_all_container_policies, load_container_policy, load_inline_workspace_container_policy,
    load_workspace_ownership_targets, resolve_inline_workspace_exec_working_dir, stats_all_report,
    status_all_report, status_report, validate_container_policy, with_test_effigy_home,
    AllocatedPortsSummary, ContainerDataTransferAction, ContainerDataVolumeEntry,
    ContainerPolicyError, ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry,
    ContainerStatusService, EffectiveAttachMode, EffectiveComposeSource, SharedServiceBinding,
};
use effigy_catalog::volumes::VolumeClassification;
use effigy_manifest::ManifestInlineWorkspaceContainerConfig;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-containers-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

fn with_temp_effigy_home<T>(name: &str, run: impl FnOnce(PathBuf) -> T) -> T {
    let home = temp_repo(&format!("home-{name}")).join(".effigy");
    fs::create_dir_all(&home).expect("mkdir effigy home");
    with_test_effigy_home(&home, || run(home.clone()))
}

#[test]
fn load_container_policy_uses_default_container() {
    let root = temp_repo("default");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.name, "web");
    assert_eq!(policy.compose_source, EffectiveComposeSource::Direct);
    assert_eq!(policy.compose_file_display, "infra/dev/docker-compose.yml");
    assert_eq!(policy.dns_domain, None);
    assert!(!policy.dns_tls);
    assert_eq!(policy.dns_port, None);
    assert_eq!(
        policy.compose_files,
        vec![root.join("infra/dev/docker-compose.yml")]
    );
}

#[test]
fn load_container_policy_uses_sole_container_without_default() {
    let root = temp_repo("sole-container");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]

[containers.web]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.name, "web");
}

#[test]
fn load_container_policy_rejects_sole_non_dev_container_without_default() {
    let root = temp_repo("sole-non-dev-container");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let error = load_container_policy(&root, None).expect_err("should fail");

    assert!(error
        .to_string()
        .contains("no sole `context = \"dev\"` container is available"));
}

#[test]
fn load_container_policy_infers_direct_compose_ports_when_manifest_ports_are_omitted() {
    let root = temp_repo("direct-compose-inferred-ports");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"
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
      - "41001:41001"
      - "127.0.0.1:41002:41002"
  mailpit:
    image: axllent/mailpit:latest
    ports:
      - target: 8025
        published: 8025
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert!(!policy.ports_declared_explicitly);
    assert!(policy
        .declared_ports
        .iter()
        .any(|value| value == "41001:41001"));
    assert!(policy
        .declared_ports
        .iter()
        .any(|value| value == "41002:41002"));
    assert!(policy
        .declared_ports
        .iter()
        .any(|value| value == "8025:8025"));
}

#[test]
fn load_inline_workspace_container_policy_writes_compose_and_derives_exec_dir() {
    let root = temp_repo("inline-workspace-policy");
    let inline = ManifestInlineWorkspaceContainerConfig {
        image: Some("node:22".to_owned()),
        mount: Some("./:/workspace".to_owned()),
        extra: Default::default(),
    };

    let policy =
        load_inline_workspace_container_policy(&root, "dev__app", &inline, None).expect("policy");
    let working_dir =
        resolve_inline_workspace_exec_working_dir(&root, "dev__app", &inline, None).expect("cwd");

    assert_eq!(policy.name, "dev__app");
    assert_eq!(policy.primary_service, "workspace");
    assert_eq!(policy.compose_source, EffectiveComposeSource::Direct);
    assert_eq!(working_dir, PathBuf::from("/workspace"));
    assert!(policy.compose_files[0].is_file(), "compose file missing");
    let compose = fs::read_to_string(&policy.compose_files[0]).expect("read compose");
    assert!(compose.contains("image: \"node:22\""), "compose: {compose}");
    assert!(
        compose.contains("working_dir: \"/workspace\""),
        "compose: {compose}"
    );
    assert!(
        compose.contains(&root.display().to_string()),
        "compose should use absolute host path: {compose}"
    );
    let dns_override = fs::read_to_string(&policy.compose_files[1]).expect("read dns override");
    assert!(
        dns_override.contains("workspace:\n    dns:"),
        "dns override should target workspace service: {dns_override}"
    );
}

#[test]
fn validate_container_policy_rejects_mounts_outside_repo() {
    let root = temp_repo("mount");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.host]
mounts = ["../outside:/workspace/outside"]
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let error = validate_container_policy(&root, &policy).expect_err("should fail");
    assert!(error.to_string().contains("escapes the repo root"));
}

#[test]
fn effective_attach_mode_respects_flags_before_policy() {
    let root = temp_repo("attach-mode");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
startup = "detached"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    assert_eq!(
        effective_attach_mode(&policy, false, false),
        EffectiveAttachMode::Detached
    );
    assert_eq!(
        effective_attach_mode(&policy, true, false),
        EffectiveAttachMode::Attached
    );
}

#[test]
fn load_container_policy_requires_registry() {
    let root = temp_repo("missing-registry");
    fs::write(root.join("effigy.toml"), "[tasks]\n").expect("write manifest");
    let error = load_container_policy(&root, None).expect_err("should fail");
    assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
    assert!(error.to_string().contains("[containers]"));
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
        assert_eq!(policy.compose_files.len(), 1);
        assert!(
            policy
                .compose_file_display
                .contains(".effigy-compose.generated.yml"),
            "display should reference generated compose, got {}",
            policy.compose_file_display
        );

        let compose_path = root.join("infra/dev/.effigy-compose.generated.yml");
        assert_eq!(policy.compose_files[0], compose_path);
        assert!(compose_path.exists(), "generated compose file should exist");

        let compose = fs::read_to_string(compose_path).expect("read generated compose");
        assert!(compose.contains("services:"));
        assert!(compose.contains("app:"));
        assert!(compose.contains("web:"));
        assert!(!policy.ports_declared_explicitly);
        let http_port = policy
            .declared_ports
            .iter()
            .filter_map(|value| value.split_once(':'))
            .find(|(_, container)| *container == "80")
            .map(|(host, _)| host.to_owned())
            .expect("generated compose should expose a host port for container port 80");
        assert!(compose.contains(&format!("{http_port}:80")));
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
        let compose = fs::read_to_string(root.join("infra/dev/.effigy-compose.generated.yml"))
            .expect("compose");

        assert!(compose.contains("18080:80"));
        assert!(compose.contains("13306:3306"));
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
            .find(|(_, container)| *container == "3306")
            .map(|(host, _)| host.parse::<u16>().expect("host port"))
            .expect("expected generated compose MySQL port");
        assert_eq!(mysql_port, http_port + 6);
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

        let compose = fs::read_to_string(root.join("infra/dev/.effigy-compose.generated.yml"))
            .expect("compose");
        assert!(!compose.contains("\n  db:\n"));
        assert!(compose.contains("DB_HOST: host.docker.internal"));
        assert!(compose.contains(&format!("DB_PORT: '{}'", shared.host_port)));

        assert!(shared.compose_file.exists());
        let shared_compose = fs::read_to_string(&shared.compose_file).expect("read shared compose");
        assert!(shared_compose.contains(&format!("{}:3306", shared.host_port)));
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
    let root = parent.join("underlay-reference");
    let underlay = parent.join("underlay");
    let poodle = parent.join("poodle");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
    fs::create_dir_all(&underlay).expect("mkdir underlay");
    fs::create_dir_all(&poodle).expect("mkdir poodle");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"
working_dir = "/workspace-root/underlay-reference"

[containers.stack.workspace]
user = "dev"
home = "/home/dev"
extra_mounts = ["../underlay", "../poodle"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  workspace:
    image: "node:22"
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

    assert!(
        rewritten.contains(":/workspace-root/underlay-reference"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains(":/workspace-root/underlay"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        rewritten.contains(":/workspace-root/poodle"),
        "rewritten compose: {rewritten}"
    );
    assert!(
        !rewritten.contains("../../../:/workspace-root"),
        "rewritten compose should remove broad parent mount: {rewritten}"
    );
    assert!(
        rewritten.contains("- stack-cache:/cache"),
        "rewritten compose should preserve named volumes: {rewritten}"
    );
    assert!(
        !rewritten.contains("user: dev"),
        "rewritten compose should not force runtime user: {rewritten}"
    );
    assert!(
        !rewritten.contains("HOME: /home/dev"),
        "rewritten compose should not force runtime HOME: {rewritten}"
    );
    assert_eq!(policy.workspace_user.as_deref(), Some("dev"));
    assert_eq!(policy.workspace_home.as_deref(), Some("/home/dev"));
}

#[test]
fn load_workspace_ownership_targets_collects_named_volume_targets() {
    let parent = std::env::temp_dir().join(format!(
        "effigy-workspace-ownership-targets-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let root = parent.join("underlay-reference");
    let underlay = parent.join("underlay");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
    fs::create_dir_all(&underlay).expect("mkdir underlay");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "stack"

[containers.stack]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"
working_dir = "/workspace-root/underlay-reference"

[containers.stack.workspace]
user = "dev"
home = "/home/dev"
extra_mounts = ["../underlay"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        r#"
services:
  workspace:
    image: "node:22"
    volumes:
      - ../../../:/workspace-root
      - stack-cache:/cache
      - stack-node-modules:/workspace-root/underlay-reference/acme-client/node_modules
      - /tmp/host-path:/workspace-root/host-cache
"#,
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let targets = load_workspace_ownership_targets(&policy).expect("targets");

    assert!(targets.iter().any(|value| value == "/home/dev"));
    assert!(targets.iter().any(|value| value == "/cache"));
    assert!(targets
        .iter()
        .any(|value| value == "/workspace-root/underlay-reference/acme-client/node_modules"));
    assert!(!targets
        .iter()
        .any(|value| value == "/workspace-root/host-cache"));
    assert!(!targets
        .iter()
        .any(|value| value == "/workspace-root/underlay-reference"));
}

#[test]
fn eject_generated_compose_promotes_generated_output_to_direct_ownership() {
    let root = temp_repo("eject-generated");
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

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");
    let generated = root.join("infra/dev/.effigy-compose.generated.yml");
    assert!(
        generated.exists(),
        "generated compose should exist before eject"
    );

    let result = eject_generated_compose(&root, &policy).expect("eject");
    assert_eq!(
        result.compose_path,
        root.join("infra/dev/docker-compose.yml")
    );
    assert!(
        result.compose_path.exists(),
        "permanent compose should exist"
    );
    assert!(!generated.exists(), "generated compose should be removed");
}

#[test]
fn eject_generated_compose_rejects_direct_compose_ownership() {
    let root = temp_repo("eject-direct");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let error = eject_generated_compose(&root, &policy).expect_err("should fail");
    assert!(error
        .to_string()
        .contains("direct `compose_file` ownership"));
}

#[test]
fn load_all_container_policies_returns_every_declared_environment() {
    let root = temp_repo("all-policies");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.worker]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "jobs"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policies = load_all_container_policies(&root).expect("policies");
    assert_eq!(policies.len(), 2);
    assert_eq!(policies[0].name, "web");
    assert_eq!(policies[1].name, "worker");
}

#[test]
fn status_all_report_renders_environment_inventory() {
    let report = status_all_report(&[ContainerStatusAllEntry {
        repo_root: "/tmp/demo".to_owned(),
        container: "web".to_owned(),
        project_name: "demo-web-dev".to_owned(),
        profile: "default".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: Some("demo.test".to_owned()),
        dns_tls: true,
        declared_ports: vec!["18080:80".to_owned()],
        allocated_ports: Some(AllocatedPortsSummary {
            base: 8100,
            http: 8100,
            mysql: 8106,
            postgres: 8132,
            redis: 8179,
            memcached: 8111,
        }),
        services: vec![ContainerStatusService {
            name: "app".to_owned(),
            container_name: "demo-app-1".to_owned(),
            status: "Up 2 minutes".to_owned(),
            ports: vec!["0.0.0.0:18080->80/tcp".to_owned()],
        }],
    }]);

    assert!(report.success_text.contains("running Effigy-managed"));
    assert!(report.success_text.contains("demo.test (tls)"));
    assert!(report.success_text.contains("allocated_ports: base=8100"));
    assert_eq!(report.json["environment_count"], 1);
    assert_eq!(report.json["environments"][0]["container"], "web");
}

#[test]
fn stats_all_report_renders_resource_inventory_and_warning() {
    let report = stats_all_report(
        &[ContainerStatsAllEntry {
            repo_root: "/tmp/demo".to_owned(),
            container: "web".to_owned(),
            project_name: "demo-web-dev".to_owned(),
            profile: "default".to_owned(),
            primary_service: "app".to_owned(),
            services: vec![
                ContainerStatsService {
                    name: "app".to_owned(),
                    container_name: "demo-app-1".to_owned(),
                    status: "Up 2 minutes".to_owned(),
                    cpu_percent: Some("1.25%".to_owned()),
                    memory_usage: Some("12.4MiB / 8GiB".to_owned()),
                    memory_percent: Some("0.15%".to_owned()),
                },
                ContainerStatsService {
                    name: "web".to_owned(),
                    container_name: "demo-web-1".to_owned(),
                    status: "Up 2 minutes".to_owned(),
                    cpu_percent: None,
                    memory_usage: None,
                    memory_percent: None,
                },
            ],
        }],
        Some("runtime stats were unavailable for: demo-web-1"),
    );

    assert_eq!(report.json["schema"], "effigy.container.stats-all.v1");
    assert_eq!(report.json["environment_count"], 1);
    assert_eq!(
        report.json["stats_warning"],
        "runtime stats were unavailable for: demo-web-1"
    );
    assert!(report
        .success_text
        .contains("[warn] runtime stats were unavailable"));
    assert!(report.success_text.contains("cpu=1.25%"));
    assert!(report.success_text.contains("memory=12.4MiB / 8GiB"));
    assert!(report.success_text.contains("cpu=unavailable"));
}

#[test]
fn status_report_renders_shared_service_targets() {
    let root = temp_repo("status-shared-services");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let mut policy = load_container_policy(&root, None).expect("policy");
    policy.shared_services = vec![SharedServiceBinding {
        service_name: "db".to_owned(),
        catalog: "mariadb".to_owned(),
        project_name: "effigy-shared-mariadb-deadbeef".to_owned(),
        compose_file: PathBuf::from("/tmp/shared/.effigy-compose.generated.yml"),
        host: "host.docker.internal".to_owned(),
        host_port: 8106,
        container_port: 3306,
    }];

    let report = status_report(&policy, true, None, None);

    assert!(report.success_text.contains("shared_services: 1"));
    assert!(report
        .success_text
        .contains("db [mariadb] -> host.docker.internal:8106"));
    assert_eq!(
        report.json["shared_services"][0]["project_name"],
        "effigy-shared-mariadb-deadbeef"
    );
}

#[test]
fn generated_compose_policy_includes_managed_volumes() {
    let root = temp_repo("managed-volumes");
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
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");

    assert!(policy
        .managed_volumes
        .iter()
        .any(|volume| volume.name.ends_with("-db-data")));
    assert!(policy.managed_volumes.iter().any(|volume| volume.persist));
}

#[test]
fn generated_compose_policy_includes_declared_media_mounts_and_prepares_source_dirs() {
    let root = temp_repo("managed-media");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.data]
media = ["storage/uploads:/var/www/html/storage/uploads"]

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"

[containers.web.services.db]
catalog = "mariadb"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(
        policy.declared_media_mounts,
        vec!["storage/uploads:/var/www/html/storage/uploads".to_owned()]
    );
    assert!(root.join("storage/uploads").is_dir());

    let compose = fs::read_to_string(&policy.compose_files[0]).expect("read compose");
    let expected = format!(
        "{}:/var/www/html/storage/uploads",
        root.join("storage/uploads").display()
    );
    assert_eq!(compose.matches(&expected).count(), 2, "compose: {compose}");
}

#[test]
fn generated_compose_policy_includes_pull_production_hook() {
    let root = temp_repo("pull-production-policy");
    fs::create_dir_all(root.join("scripts")).expect("mkdir scripts");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.data]
pull_production = "scripts/pull-production.sh"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
"#,
    )
    .expect("write manifest");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(
        policy.pull_production_hook.as_deref(),
        Some("scripts/pull-production.sh")
    );
}

#[test]
fn direct_compose_policy_rejects_media_mounts() {
    let root = temp_repo("direct-media-reject");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.data]
media = ["storage/uploads:/var/www/html/storage/uploads"]
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let error = load_container_policy(&root, None).expect_err("should fail");
    assert!(error
        .to_string()
        .contains("bounded media lifecycle path only supports generated compose"));
}

#[test]
fn reset_report_renders_keep_data_volume_actions() {
    let root = temp_repo("reset-report");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let report = super::reset_report(
        &policy,
        true,
        true,
        Some(&VolumeClassification {
            keep: vec!["demo-web-dev-db-data".to_owned()],
            remove: vec!["demo-web-dev-cache-data".to_owned()],
        }),
    );

    assert!(report
        .success_text
        .contains("preserved persistent data volumes"));
    assert!(report
        .success_text
        .contains("kept_volumes: demo-web-dev-db-data"));
    assert!(report
        .success_text
        .contains("removed_volumes: demo-web-dev-cache-data"));
    assert_eq!(report.json["keep_data"], true);
    assert_eq!(report.json["volumes"]["kept"][0], "demo-web-dev-db-data");
    assert_eq!(
        report.json["volumes"]["removed"][0],
        "demo-web-dev-cache-data"
    );
}

#[test]
fn data_list_report_renders_volume_inventory_and_unavailable_metadata() {
    let root = temp_repo("data-list-report");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let report = data_list_report(
        &policy,
        false,
        &[
            ContainerDataVolumeEntry {
                name: "demo-web-dev-db-data".to_owned(),
                service: "db".to_owned(),
                persist: true,
                size_bytes: Some(2048),
                mount_point: Some("/var/lib/docker/volumes/demo-web-dev-db-data/_data".to_owned()),
            },
            ContainerDataVolumeEntry {
                name: "demo-web-dev-app-node-modules".to_owned(),
                service: "app".to_owned(),
                persist: false,
                size_bytes: None,
                mount_point: None,
            },
        ],
    );

    assert_eq!(report.json["schema"], "effigy.container.data-list.v1");
    assert_eq!(report.json["volume_count"], 2);
    assert_eq!(report.json["volumes"][0]["classification"], "persistent");
    assert_eq!(report.json["volumes"][1]["classification"], "ephemeral");
    assert!(report
        .success_text
        .contains("runtime_metadata: unavailable"));
    assert!(report.success_text.contains("size=2.0 KiB"));
    assert!(report.success_text.contains("size=unavailable"));
}

#[test]
fn data_transfer_report_renders_export_contract() {
    let root = temp_repo("data-transfer-report");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let report = data_transfer_report(
        &policy,
        ContainerDataTransferAction::Export,
        &ContainerDataVolumeEntry {
            name: "demo-web-dev-db-data".to_owned(),
            service: "db".to_owned(),
            persist: true,
            size_bytes: None,
            mount_point: None,
        },
        Path::new("/tmp/demo-backup.tar.gz"),
    );

    assert_eq!(report.json["schema"], "effigy.container.data-export.v1");
    assert_eq!(report.json["action"], "export");
    assert_eq!(report.json["volume"]["name"], "demo-web-dev-db-data");
    assert_eq!(report.json["output_path"], "/tmp/demo-backup.tar.gz");
    assert!(report.success_text.contains("exported managed volume"));
}

#[test]
fn data_pull_production_report_renders_hook_contract() {
    let root = temp_repo("pull-production-report");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let report = super::data_pull_production_report(
        &policy,
        &super::ContainerDataHookResult {
            hook: "rhai:scripts/pull-prod.rhai".to_owned(),
        },
        true,
        Some("ready"),
    );

    assert_eq!(
        report.json["schema"],
        "effigy.container.data-pull-production.v1"
    );
    assert_eq!(report.json["hook"], "rhai:scripts/pull-prod.rhai");
    assert_eq!(report.json["colima_started"], true);
    assert!(report
        .success_text
        .contains("ran production data hook for container `web`"));
}

#[test]
fn status_report_renders_media_mounts() {
    let root = temp_repo("status-media");
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
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let mut policy = load_container_policy(&root, None).expect("policy");
    policy.declared_media_mounts = vec!["storage/uploads:/var/www/html/storage/uploads".to_owned()];

    let report = status_report(&policy, true, None, None);

    assert_eq!(
        report.json["media_mounts"][0],
        "storage/uploads:/var/www/html/storage/uploads"
    );
    assert!(report
        .success_text
        .contains("media_mounts: storage/uploads:/var/www/html/storage/uploads"));
}
