use super::*;

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
    let expected = root
        .file_name()
        .and_then(|value| value.to_str())
        .expect("repo dir name")
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    assert_eq!(policy.project_name, format!("{expected}-dev"));
}

#[test]
fn load_container_exec_working_dir_ignores_host_task_defaults_for_workspace_inference() {
    let root = temp_repo("bundle-host-default-workdir");
    let manifest_path = root.join("effigy.toml");
    fs::write(
        &manifest_path,
        r#"
[bundle]
base = "underlay"
host = "acme.test"
project_name = "underlay-reference-dev"
workspace_subdir = "underlay-reference"
database = "acme"

[systems.dev]
mounts = ["../underlay"]

[task_defaults]
run_in = "host"
"#,
    )
    .expect("write manifest");
    fs::create_dir(root.join(".git")).expect("git dir");

    let working_dir = load_container_exec_working_dir(&root, Some("stack")).expect("working dir");

    assert_eq!(
        working_dir,
        PathBuf::from("/workspace-root/underlay-reference")
    );
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
fn load_container_policy_uses_catalog_alias_as_single_container_project_name() {
    let root = temp_repo("single-container-alias");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "underlay-reference"

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

    assert_eq!(policy.project_name, "underlay-reference-dev");
}

#[test]
fn load_container_policy_rejects_duplicate_effective_project_names() {
    let root = temp_repo("duplicate-project-names");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "underlay-reference"

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
project_name = "underlay-reference-dev"

[containers.worker]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "jobs"
project_name = "underlay-reference-dev"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

    let error = load_container_policy(&root, None).expect_err("should fail");

    assert!(error.to_string().contains("unique `project_name` values"));
    assert!(error.to_string().contains("`underlay-reference-dev`"));
    assert!(error.to_string().contains("`web`"));
    assert!(error.to_string().contains("`worker`"));
}

#[test]
fn load_container_policy_uses_distinct_default_project_names_for_multiple_containers() {
    let root = temp_repo("multiple-default-project-names");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "underlay-reference"

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
    assert_eq!(policies[0].project_name, "underlay-reference-web-dev");
    assert_eq!(policies[1].name, "worker");
    assert_eq!(policies[1].project_name, "underlay-reference-worker-dev");
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
    fs::create_dir(root.join(".git")).expect("git dir");
    let inline = ManifestInlineWorkspaceContainerConfig {
        image: Some("node:22".to_owned()),
        mount: Some("./:/workspace".to_owned()),
        extra: Default::default(),
    };

    let policy =
        load_inline_workspace_container_policy(&root, "dev__app", &inline, None).expect("policy");
    assert_eq!(
        fs::read_to_string(root.join(".gitignore")).expect("gitignore"),
        ".effigy\n"
    );
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
    let parent = std::env::temp_dir().join(format!(
        "effigy-mount-validation-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let root = parent.join("repo");
    let outside = parent.join("outside");
    fs::create_dir_all(&root).expect("mkdir repo");
    fs::create_dir_all(&outside).expect("mkdir outside");
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

    // Host-mount resolution moved to intake (see `mount_spec`), so the
    // repo-escape check now fires at policy-load time, not at later
    // `validate_container_policy`.
    let error = load_container_policy(&root, None).expect_err("should fail");
    assert!(error.to_string().contains("escapes the repo root"));
}

#[test]
fn validate_compose_backend_runtime_rejects_temp_root_repo_for_colima_nerdctl() {
    let root = temp_repo("temp-root-colima-compose");
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
    let error = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
        crate::validate_compose_backend_runtime(&root, &policy).expect_err("should fail")
    });

    assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
    assert!(error
        .to_string()
        .contains("under a temp directory that Colima may not share"));
    assert!(error.to_string().contains("/Users/..."));
}

#[test]
fn validate_compose_backend_runtime_rejects_colima_mount_payload_over_budget() {
    let root = temp_repo("colima-mount-budget");
    let volumes = (0..72)
        .map(|index| {
            format!(
                "      - workspace-volume-{index:02}:/workspace-root/acme/some/really/long/path/{index:02}/node_modules\n"
            )
        })
        .collect::<String>();
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
        format!(
            "services:\n  app:\n    image: alpine\n    volumes:\n{volumes}volumes:\n  workspace-volume-00: {{}}\n"
        ),
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");
    let previous = std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK");
    unsafe {
        std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", "1");
    }
    let error = with_test_compose_backend(ComposeBackend::ColimaNerdctl, || {
        crate::validate_compose_backend_runtime(&root, &policy).expect_err("should fail")
    });
    match previous {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK", value)
        },
        None => unsafe { std::env::remove_var("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK") },
    }

    assert!(matches!(error, ContainerPolicyError::TaskInvocation(_)));
    assert!(error.to_string().contains("estimated mount payload"));
    assert!(error
        .to_string()
        .contains("trim isolation or workspace mounts"));
    assert!(error
        .to_string()
        .contains("/workspace-root/acme/some/really/long/path"));
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
