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

fn setup_workspace_app_path_bundle(root: &std::path::Path) {
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../effigy-manifest/tests/fixtures/workspace-app-bundle");
    let bundle_dir = root.join("bundles/workspace-app");
    copy_dir_all(&fixture_dir, &bundle_dir).expect("copy fixture");
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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.name, "web");
    assert_eq!(policy.compose_source, EffectiveComposeSource::Generated);
    assert_eq!(
        policy.compose_file_display,
        ".effigy/runtime/compose/web.generated.compose.yml"
    );
    assert_eq!(policy.dns_domain, None);
    assert!(!policy.dns_tls);
    assert_eq!(policy.dns_port, None);
    let render = policy.render.as_ref().unwrap();
    assert_eq!(
        policy.compose_files,
        vec![
            render
                .path
                .join("checkout/.effigy/runtime/compose/web.generated.compose.yml"),
            render
                .path
                .join("checkout/.effigy/runtime/dns/web.compose.override.yml"),
        ]
    );
}

#[test]
fn load_container_policy_falls_back_to_the_default_system_workspace_container() {
    let root = temp_repo("system-default");
    fs::write(
        root.join("effigy.toml"),
        r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.name, "web");
}

#[test]
fn load_container_policy_prefers_the_container_default_over_the_system_binding() {
    let root = temp_repo("container-default-before-system");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "api"

[containers.api]
compose_file = "infra/dev/api-compose.yml"
primary_service = "app"

[containers.web]
compose_file = "infra/dev/web-compose.yml"
primary_service = "app"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/api-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");
    fs::write(
        root.join("infra/dev/web-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.name, "api");
}

#[test]
fn creation_policy_resolves_direct_workspace_mounts_after_preparation() {
    let root = temp_repo("workspace-mount-create-direct");
    let absolute_source = root.join("absolute-extra");
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = [
  "late:/workspace-root/late:ro,cached",
  {{ source = "{}", target = "/workspace-root/absolute-extra", options = ["ro"] }}
]
"#,
            absolute_source.display()
        ),
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let initially_loaded = load_container_policy(&root, Some("web")).expect("initial policy");
    let initial_compose = initially_loaded
        .read_compose(&initially_loaded.compose_files[0])
        .expect("initial workspace compose");
    assert!(!initial_compose.contains("/workspace-root/late"));
    assert!(!initial_compose.contains("/workspace-root/absolute-extra"));

    fs::create_dir(root.join("late")).expect("preparation creates late source");
    fs::create_dir(&absolute_source).expect("preparation creates absolute source");

    let (creation_policy, decisions) =
        load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
    let created_compose = creation_policy
        .read_compose(&creation_policy.compose_files[0])
        .expect("creation workspace compose");
    let canonical_late = root.join("late").canonicalize().expect("canonical late");
    let canonical_absolute = absolute_source.canonicalize().expect("canonical absolute");

    assert!(created_compose.contains(&format!(
        "{}:/workspace-root/late:ro,cached",
        canonical_late.display()
    )));
    assert!(created_compose.contains(&format!(
        "{}:/workspace-root/absolute-extra:ro",
        canonical_absolute.display()
    )));
    assert_eq!(decisions.len(), 2);
    assert!(decisions.iter().all(|decision| {
        decision.decision == WorkspaceExtraMountDecisionKind::IncludedInComposeSpec
    }));
    assert!(decisions.iter().any(|decision| {
        decision.source == canonical_late.display().to_string()
            && decision.target == "/workspace-root/late"
    }));
    assert!(decisions.iter().any(|decision| {
        decision.source == canonical_absolute.display().to_string()
            && decision.target == "/workspace-root/absolute-extra"
    }));
}

#[test]
fn creation_policy_skips_removed_optional_source_without_creating_it() {
    let root = temp_repo("workspace-mount-create-removed");
    let source = root.join("early");
    fs::create_dir(&source).expect("create initial source");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["early:/workspace-root/early:ro"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let _initial = load_container_policy(&root, Some("web")).expect("initial policy");
    fs::remove_dir_all(&source).expect("preparation removes source");

    let (creation_policy, decisions) =
        load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
    let created_compose = creation_policy
        .read_compose(&creation_policy.compose_files[0])
        .expect("creation workspace compose");

    assert!(
        !source.exists(),
        "resolver must not recreate the host source"
    );
    assert!(!created_compose.contains("/workspace-root/early"));
    assert_eq!(decisions.len(), 1);
    assert_eq!(
        decisions[0].decision,
        WorkspaceExtraMountDecisionKind::SkippedSourceAbsent
    );
    assert_eq!(
        decisions[0].source,
        source
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .join(source.file_name().unwrap())
            .display()
            .to_string()
    );
    assert_eq!(decisions[0].target, "/workspace-root/early");
}

#[cfg(unix)]
#[test]
fn creation_policy_canonicalizes_optional_symlink_sources() {
    use std::os::unix::fs::symlink;

    let root = temp_repo("workspace-mount-create-symlink");
    let real_source = root.join("actual-source");
    let link_source = root.join("linked-source");
    fs::create_dir(&real_source).expect("create source");
    symlink(&real_source, &link_source).expect("create source symlink");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["linked-source:/workspace-root/linked:ro"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let (policy, decisions) =
        load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
    let compose = policy
        .read_compose(&policy.compose_files[0])
        .expect("read compose");
    let canonical = real_source.canonicalize().expect("canonical source");

    assert!(compose.contains(&format!(
        "{}:/workspace-root/linked:ro",
        canonical.display()
    )));
    assert!(!compose.contains(&format!(
        "{}:/workspace-root/linked:ro",
        link_source.display()
    )));
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].source, canonical.display().to_string());
}

#[test]
fn creation_policy_refuses_unreadable_optional_source_instead_of_skipping() {
    let root = temp_repo("workspace-mount-create-unreadable");
    fs::write(root.join("blocked"), "not a directory").expect("write blocking file");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "compose.yml"
primary_service = "app"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
working_dir = "/workspace-root/app"
mounts = ["blocked/source:/workspace-root/blocked:ro"]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("compose.yml"),
        "services:\n  app:\n    image: alpine:latest\n",
    )
    .expect("write compose");

    let error = load_container_policy_for_creation(&root, Some("web"))
        .expect_err("ENOTDIR is not evidence that the optional source is absent");

    assert!(error.to_string().contains("workspace extra mount source"));
    assert!(!root.join("blocked/source").exists());
}

#[test]
fn creation_policy_resolves_generated_workspace_mounts_after_preparation() {
    with_temp_effigy_home("workspace-mount-create-generated", |_| {
        let _lock = crate::test_env_lock();
        let root = temp_repo("workspace-mount-create-generated");
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
mounts = ["late:/var/www/late:ro"]
"#,
        )
        .expect("write manifest");

        let initial = load_container_policy(&root, Some("web")).expect("initial policy");
        assert_eq!(initial.compose_source, EffectiveComposeSource::Generated);
        fs::create_dir(root.join("late")).expect("preparation creates source");

        let (creation_policy, decisions) =
            load_container_policy_for_creation(&root, Some("web")).expect("creation policy");
        let compose = creation_policy
            .read_compose(&creation_policy.compose_files[0])
            .expect("creation workspace compose");
        let canonical = root.join("late").canonicalize().expect("canonical source");
        let backend = crate::ContainerManager::defaults();
        let request = crate::ContainerManagerRequest::new(&root)
            .backend_override(crate::BackendId::docker_compose());
        let plan = backend
            .compose_invocation_plan(
                &request,
                &crate::ContainerBackendDetection::new(false),
                &creation_policy.profile,
                &crate::compose::compose_up_args(&creation_policy),
                crate::ContainerAction::Activate,
                "docker compose up",
            )
            .expect("generated compose backend plan");
        let captured_compose = plan
            .args
            .windows(2)
            .find(|args| args[0] == "-f")
            .map(|args| PathBuf::from(&args[1]))
            .expect("backend plan captures generated compose file");

        assert!(compose.contains(&format!("{}:/var/www/late:ro", canonical.display())));
        assert_eq!(captured_compose, creation_policy.compose_files[0]);
        assert!(fs::read_to_string(captured_compose)
            .expect("read captured generated compose")
            .contains(&format!("{}:/var/www/late:ro", canonical.display())));
        assert_eq!(decisions.len(), 1);
        assert_eq!(
            decisions[0].decision,
            WorkspaceExtraMountDecisionKind::IncludedInComposeSpec
        );
    });
}

#[test]
fn load_container_policy_requires_a_default_when_name_omitted() {
    let root = temp_repo("sole-container");
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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let error = load_container_policy(&root, None).expect_err("should fail");

    assert!(error
        .to_string()
        .contains("container name omitted but neither `[containers].default` nor a `[systems].default` workspace container is defined"));
}

#[test]
fn load_container_exec_working_dir_ignores_host_task_defaults_for_workspace_inference() {
    let root = temp_repo("bundle-host-default-workdir");
    setup_workspace_app_path_bundle(&root);
    let manifest_path = root.join("effigy.toml");
    fs::write(
        &manifest_path,
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "workspace-app-reference-dev"
workspace_subdir = "workspace-app-reference"
databases = ["acme"]

[systems.dev]
mounts = ["../platform"]

[task_defaults]
run_in = "host"
"#,
    )
    .expect("write manifest");
    fs::create_dir(root.join(".git")).expect("git dir");

    let working_dir = load_container_exec_working_dir(&root, Some("stack")).expect("working dir");

    assert_eq!(
        working_dir,
        PathBuf::from("/workspace-root/workspace-app-reference")
    );
}

#[test]
fn load_container_policy_resolves_named_member_mounts() {
    let root = temp_repo("named-member-mount");
    setup_workspace_app_path_bundle(&root);
    fs::create_dir_all(root.join("underlay")).expect("member dir");
    fs::write(
        root.join("effigy.toml"),
        r#"
[bundle]
base = { type = "path", dir = "bundles/workspace-app" }
host = "acme.test"
project_name = "workspace-app-reference-dev"
workspace_subdir = "workspace-app-reference"
databases = ["acme"]

[catalog.members]
underlay = "underlay"

[systems.dev]
mounts = [{ member = "underlay", target = "/workspace-root/underlay" }]
"#,
    )
    .expect("write manifest");
    fs::create_dir(root.join(".git")).expect("git dir");

    let policy = {
        let _lock = crate::test_env_lock();
        load_container_policy(&root, Some("stack")).expect("policy")
    };

    let rewritten = policy
        .read_compose(&policy.compose_files[0])
        .expect("rewritten compose");
    assert!(
        rewritten.contains(
            &root
                .join("underlay")
                .canonicalize()
                .unwrap()
                .display()
                .to_string()
        ),
        "named member source should be rendered into compose: {rewritten}"
    );
}

#[test]
fn load_container_policy_accepts_named_container_without_default() {
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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, Some("web")).expect("policy");

    assert_eq!(policy.name, "web");
}

#[test]
fn load_container_policy_uses_catalog_alias_as_single_container_project_name() {
    let root = temp_repo("single-container-alias");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "workspace-app-reference"

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
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.project_name, "workspace-app-reference-dev");
}

#[test]
fn load_container_policy_sanitizes_default_project_name_components() {
    let root = temp_repo("R7-webCore");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "R7-webCore"

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
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.project_name, "r7-webcore-dev");
}

#[test]
fn load_container_policy_sanitizes_explicit_project_name() {
    let root = temp_repo("explicit-project-name");
    fs::write(
        root.join("effigy.toml"),
        r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
project_name = "r7-webCore-decodelabs-library"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.project_name, "r7-webcore-decodelabs-library");
}

#[test]
fn load_container_policy_uses_repo_name_when_alias_only_comes_from_bundle_defaults() {
    let root = temp_repo("single-container-bundle-root-alias");
    let bundle = root.with_file_name("single-container-bundle-root-alias-bundle");
    fs::create_dir_all(&bundle).expect("mkdir bundle");
    fs::write(
        bundle.join("bundle.toml"),
        r#"
[bundle]
name = "legacy-site"
description = "legacy site fixture"
"#,
    )
    .expect("write bundle descriptor");
    fs::write(
        bundle.join("effigy.toml"),
        r#"
[catalog]
alias = "root"
"#,
    )
    .expect("write bundle defaults");
    fs::write(
        root.join("effigy.toml"),
        format!(
            r#"
[bundle]
base = {{ type = "path", dir = "{}" }}

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
            bundle.display()
        ),
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = {
        let _lock = crate::test_env_lock();
        load_container_policy(&root, None).expect("policy")
    };
    let expected = format!(
        "{}-dev",
        root.file_name().and_then(|name| name.to_str()).unwrap()
    );

    assert_eq!(policy.project_name, expected);
}

#[test]
fn load_container_policy_rejects_duplicate_effective_project_names() {
    let root = temp_repo("duplicate-project-names");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "workspace-app-reference"

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
project_name = "workspace-app-reference-dev"

[containers.worker]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "jobs"
project_name = "workspace-app-reference-dev"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let error = load_container_policy(&root, None).expect_err("should fail");

    assert!(error.to_string().contains("unique `project_name` values"));
    assert!(error.to_string().contains("`workspace-app-reference-dev`"));
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
alias = "workspace-app-reference"

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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policies = load_all_container_policies(&root).expect("policies");

    assert_eq!(policies.len(), 2);
    assert_eq!(policies[0].name, "web");
    assert_eq!(policies[0].project_name, "workspace-app-reference-web-dev");
    assert_eq!(policies[1].name, "worker");
    assert_eq!(
        policies[1].project_name,
        "workspace-app-reference-worker-dev"
    );
}

#[test]
fn load_container_policy_appends_bootstrap_fresh_session_suffix() {
    let root = temp_repo("fresh-session-project-name");
    fs::write(
        root.join("effigy.toml"),
        r#"
[catalog]
alias = "contact-patch"

[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
    )
    .expect("write manifest");
    fs::create_dir_all(root.join(".effigy/runtime")).expect("mkdir session dir");
    fs::write(
        root.join(".effigy/runtime/bootstrap-fresh-session.json"),
        r#"{
  "session_id": "fresh-abcd1234",
  "root_repo": "/tmp/contact-patch",
  "repos": ["/tmp/contact-patch"],
  "active": true
}
"#,
    )
    .expect("write session record");
    fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    let policy = load_container_policy(&root, None).expect("policy");

    assert_eq!(policy.project_name, "contact-patch-dev-fresh-abcd1234");
}

#[test]
fn translated_compose_ports_follow_catalog_port_policy() {
    let _lock = crate::test_env_lock();
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

    // Translated ports follow catalog port policy: every published container
    // port is kept, and Prepare assigns the host side.
    assert!(!policy.ports_declared_explicitly);
    for container in ["41001", "41002", "8025"] {
        assert!(
            policy
                .declared_ports
                .iter()
                .any(|value| value.ends_with(&format!(":{container}"))),
            "{:?}",
            policy.declared_ports
        );
    }
}

#[test]
fn load_inline_workspace_container_policy_writes_compose_and_derives_exec_dir() {
    let _lock = crate::test_env_lock();
    let root = temp_repo("inline-workspace-policy");
    fs::create_dir(root.join(".git")).expect("git dir");
    let inline = ManifestInlineWorkspaceContainerConfig {
        image: Some("node:22".to_owned()),
        mount: Some("./:/workspace".to_owned()),
        extra: Default::default(),
    };

    let policy =
        load_inline_workspace_container_policy(&root, "dev__app", &inline, None).expect("policy");
    assert!(!root.join(".gitignore").exists());
    assert!(
        !root.join(".git/info/exclude").exists(),
        "resolution must not edit Git exclusions"
    );
    let working_dir =
        resolve_inline_workspace_exec_working_dir(&root, "dev__app", &inline, None).expect("cwd");

    assert_eq!(policy.name, "dev__app");
    assert_eq!(policy.primary_service, "workspace");
    assert_eq!(policy.compose_source, EffectiveComposeSource::Direct);
    assert_eq!(working_dir, PathBuf::from("/workspace"));
    assert!(
        !policy.compose_files[0].exists(),
        "inline resolution must not publish files"
    );
    let compose = policy
        .read_compose(&policy.compose_files[0])
        .expect("read compose");
    assert!(compose.contains("image: node:22"), "compose: {compose}");
    assert!(
        compose.contains("working_dir: /workspace"),
        "compose: {compose}"
    );
    assert!(
        compose.contains(&root.canonicalize().unwrap().display().to_string()),
        "compose should use absolute host path: {compose}"
    );
    let dns_override = policy
        .read_compose(&policy.compose_files[1])
        .expect("read dns override");
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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

    // Host-mount resolution moved to intake (see `mount_spec`), so the
    // repo-escape check now fires at policy-load time, not at later
    // `validate_container_policy`.
    let error = load_container_policy(&root, None).expect_err("should fail");
    assert!(error.to_string().contains("escapes the repo root"));
}

#[test]
fn validate_compose_backend_runtime_rejects_temp_root_repo_for_colima_nerdctl() {
    // Backend resolution reads `HOME` and `EFFIGY_COMPOSE_BACKEND`, which
    // sibling tests swap process-globally. Take the same lock they hold.
    let _lock = crate::test_env_lock();
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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

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
    let _lock = crate::test_env_lock();
    let root = temp_repo("colima-mount-budget");
    let volumes = (0..72)
        .map(|index| {
            format!(
                "      - workspace-volume-{index:02}:/workspace-root/acme/some/really/long/path/{index:02}/node_modules\n"
            )
        })
        .collect::<String>();
    let declared = (0..72)
        .map(|index| format!("  workspace-volume-{index:02}: {{}}\n"))
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
            "services:\n  app:\n    image: alpine\n    volumes:\n{volumes}volumes:\n{declared}"
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
    fs::write(
        root.join("infra/dev/docker-compose.yml"),
        "services:\n  app:\n    image: example\n",
    )
    .expect("compose");

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
