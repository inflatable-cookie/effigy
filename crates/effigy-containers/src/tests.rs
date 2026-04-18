use super::{
    effective_attach_mode, eject_generated_compose, load_container_policy,
    validate_container_policy, ContainerPolicyError, EffectiveAttachMode, EffectiveComposeSource,
};
use std::fs;
use std::path::PathBuf;

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
    let root = temp_repo("catalog-services");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.dns]
domain = "clientname.test"
tls = true
port = 8080

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
