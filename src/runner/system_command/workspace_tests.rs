use super::*;
use crate::contract_test_support::EnvGuard;
use std::sync::atomic::{AtomicU64, Ordering};

fn temp_repo(manifest: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-system-tests-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir repo");
    std::fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
    std::fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n")
        .expect("write compose");
    root
}

#[test]
fn permission_command_tolerates_read_only_bind_mounts() {
    let cmd =
        render_workspace_permission_command("dev", &["/home/dev".to_owned(), "/cache".to_owned()]);
    assert!(
        cmd.contains("chown -fR"),
        "permission prep should use `chown -fR` so the per-entry error \
         message on read-only host bind mounts is suppressed:\n{cmd}"
    );
    assert!(
        cmd.contains("|| true"),
        "permission prep should tolerate per-entry chown failures via \
         `|| true` — `chown -f` only hides the message, it still exits \
         non-zero when a read-only bind mount can't be chowned:\n{cmd}"
    );
    assert!(
        !cmd.contains("chown -R "),
        "permission prep must not fall back to plain `chown -R` — that \
         dies on read-only host bind mounts under /home/dev:\n{cmd}"
    );
    assert!(cmd.contains("'/home/dev'"));
    assert!(cmd.contains("'/cache'"));
}

#[test]
fn workspace_handoff_notice_mentions_container() {
    let rendered = render_workspace_handoff_notice(&EffectiveContainerPolicy {
        name: "stack".to_owned(),
        driver: effigy_manifest::ManifestContainerDriver::Colima,
        startup: effigy_manifest::ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: effigy_containers::EffectiveComposeSource::Direct,
        compose_files: vec![std::path::PathBuf::from("docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        shared_services: vec![],
        project_name: "demo-stack".to_owned(),
        primary_service: "workspace".to_owned(),
        dns_domain: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: vec![],
        service_aliases: vec![],
        declared_ports: vec![],
        ports_declared_explicitly: false,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
        shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: Vec::new(),
    });

    assert!(rendered.contains("[next]"));
    assert!(rendered.contains("switching into workspace container `stack`"));
}

#[test]
fn workspace_info_render_mentions_info_label_and_message() {
    let rendered = render_workspace_info("building linux effigy artifact");

    assert!(rendered.contains("[info]"));
    assert!(rendered.contains("building linux effigy artifact"));
}

#[test]
fn workspace_handoff_reset_sequence_clears_screen_and_scrollback() {
    assert_eq!(
        workspace_handoff_terminal_reset_sequence(),
        "\x1b[2J\x1b[H\x1b[3J"
    );
}

#[test]
fn resolve_public_workspace_container_uses_implied_default_workspace() {
    let root = temp_repo(
        r#"
[catalog]
alias = "probe"

[containers.app]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "workspace"

[systems.dev]
"#,
    );

    let container = resolve_public_workspace_container(&root, None, None, "workspace")
        .expect("resolve workspace container");

    assert_eq!(container.as_deref(), Some("app"));
}

#[test]
fn linux_artifact_refreshes_when_missing_or_older_than_host_binary() {
    let root = std::env::temp_dir().join(format!(
        "effigy-linux-artifact-refresh-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("mkdir root");
    let host = root.join("effigy-host");
    let artifacts_dir = root.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir).expect("mkdir artifacts");
    let artifact = artifacts_dir.join("effigy-linux");
    let receipt = artifacts_dir.join("rehearsal.txt");
    std::fs::write(&host, "host").expect("write host");

    assert!(linux_workspace_effigy_artifact_needs_refresh(
        &host,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    ));

    std::fs::write(&artifact, "artifact").expect("write artifact");
    std::fs::write(
        &receipt,
        "release_triple=x86_64-unknown-linux-gnu\ncompleted_at=2026-04-27T00:00:00Z\n",
    )
    .expect("write receipt");
    assert!(!linux_workspace_effigy_artifact_needs_refresh(
        &host,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    ));

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&host, "host-new").expect("rewrite host");
    assert!(linux_workspace_effigy_artifact_needs_refresh(
        &host,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    ));
}

#[test]
fn linux_artifact_refreshes_when_rehearsal_receipt_is_missing_or_wrong_target() {
    let root = std::env::temp_dir().join(format!(
        "effigy-linux-artifact-receipt-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let artifacts_dir = root.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir).expect("mkdir artifacts");
    let host = root.join("effigy-host");
    let artifact = artifacts_dir.join("effigy-linux");
    let receipt = artifacts_dir.join("rehearsal.txt");
    std::fs::write(&host, "host").expect("write host");
    std::fs::write(&artifact, "artifact").expect("write artifact");

    assert!(linux_workspace_effigy_artifact_needs_refresh(
        &host,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    ));

    std::fs::write(
        &receipt,
        "release_triple=aarch64-unknown-linux-gnu\ncompleted_at=2026-04-27T00:00:00Z\n",
    )
    .expect("write wrong-target receipt");
    assert!(linux_workspace_effigy_artifact_needs_refresh(
        &host,
        &artifact,
        LinuxWorkspaceTarget::X86_64Gnu,
    ));
}

#[test]
fn discover_effigy_repo_root_walks_up_to_repo_markers() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-root-discovery-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("tasks")).expect("mkdir tasks");
    std::fs::create_dir_all(root.join("containers")).expect("mkdir containers");
    std::fs::create_dir_all(root.join("target/debug")).expect("mkdir debug");
    std::fs::write(root.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(root.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
    std::fs::write(root.join("containers/effigy.containers.toml"), "").expect("write containers");

    let discovered = discover_effigy_repo_root(Some(root.join("target/debug").as_path()))
        .expect("discover repo root");
    assert_eq!(discovered, root);
}

#[test]
fn linux_workspace_release_url_matches_published_artifact_shape() {
    assert_eq!(
        linux_workspace_effigy_release_url(LinuxWorkspaceTarget::X86_64Gnu),
        format!(
            "https://github.com/inflatable-cookie/effigy/releases/download/v{}/effigy-x86_64-unknown-linux-gnu",
            env!("CARGO_PKG_VERSION")
        )
    );
    assert_eq!(
        linux_workspace_effigy_release_url(LinuxWorkspaceTarget::Aarch64Gnu),
        format!(
            "https://github.com/inflatable-cookie/effigy/releases/download/v{}/effigy-aarch64-unknown-linux-gnu",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn local_workspace_effigy_freshness_anchor_prefers_newest_repo_local_build() {
    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-anchor-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let host = root.join("outside-effigy");
    let target_debug = root.join("target/debug/effigy");
    let target_bootstrap = root.join("target/bootstrap-local/debug/effigy");
    let local_install = root.join(".local-install/bin/effigy");
    for path in [&target_debug, &target_bootstrap, &local_install] {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir parent");
    }
    std::fs::write(&host, "host").expect("write host");
    std::fs::write(&target_debug, "debug").expect("write target debug");
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&target_bootstrap, "bootstrap").expect("write target bootstrap");
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(&local_install, "local-install").expect("write local install");

    let anchor = resolve_local_workspace_effigy_freshness_anchor(&host, &root);
    assert_eq!(anchor, local_install);
}

#[test]
fn workspace_artifact_source_defaults_to_auto() {
    let _env = EnvGuard::set_many(&[(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, None)]);
    assert_eq!(
        configured_linux_workspace_artifact_source().expect("artifact source"),
        LinuxWorkspaceArtifactSource::Auto
    );
}

#[test]
fn workspace_artifact_source_accepts_download_aliases() {
    for value in ["download", "github", "release"] {
        let _env =
            EnvGuard::set_many(&[(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, Some(value.to_owned()))]);
        assert_eq!(
            configured_linux_workspace_artifact_source().expect("artifact source"),
            LinuxWorkspaceArtifactSource::Download
        );
    }
}

#[test]
fn workspace_artifact_source_rejects_unknown_values() {
    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("weird".to_owned()),
    )]);
    let error = configured_linux_workspace_artifact_source().expect_err("unknown artifact source");
    let rendered = error.to_string();
    assert!(rendered.contains(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV));
    assert!(rendered.contains("auto"));
    assert!(rendered.contains("download"));
}

#[test]
fn workspace_artifact_source_download_bypasses_discoverable_local_repo() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-source-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-source-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("consumer");
    let local_effigy = root.join("effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(local_effigy.join("tasks")).expect("mkdir tasks");
    std::fs::create_dir_all(local_effigy.join("containers")).expect("mkdir containers");
    std::fs::write(local_effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(local_effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
    std::fs::write(local_effigy.join("containers/effigy.containers.toml"), "")
        .expect("write containers");
    let local_artifact =
        local_effigy.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    std::fs::create_dir_all(local_artifact.parent().expect("artifact parent"))
        .expect("mkdir artifact parent");
    std::fs::write(&local_artifact, "local-artifact").expect("write local artifact");

    let cache_path =
        linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");
    std::fs::create_dir_all(cache_path.parent().expect("cache parent"))
        .expect("mkdir cache parent");
    std::fs::write(&cache_path, "downloaded-artifact").expect("write cache artifact");

    let _env = EnvGuard::set_many(&[(
        EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV,
        Some("download".to_owned()),
    )]);
    let artifact =
        ensure_linux_workspace_effigy_artifact(&workspace, LinuxWorkspaceTarget::X86_64Gnu)
            .expect("resolve artifact");

    assert_eq!(artifact, cache_path);
}

#[test]
fn resolve_local_effigy_repo_root_ignores_cached_download_artifact_in_auto_mode() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-auto-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let root = std::env::temp_dir().join(format!(
        "effigy-workspace-artifact-auto-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("underlay-reference");
    let local_effigy = root.join("effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(local_effigy.join("tasks")).expect("mkdir tasks");
    std::fs::create_dir_all(local_effigy.join("containers")).expect("mkdir containers");
    std::fs::write(local_effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(local_effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
    std::fs::write(local_effigy.join("containers/effigy.containers.toml"), "")
        .expect("write containers");
    let local_artifact =
        local_effigy.join(LinuxWorkspaceTarget::X86_64Gnu.artifact_relative_path());
    std::fs::create_dir_all(local_artifact.parent().expect("artifact parent"))
        .expect("mkdir artifact parent");
    std::fs::write(&local_artifact, "local-artifact").expect("write local artifact");

    let cache_path =
        linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");
    std::fs::create_dir_all(cache_path.parent().expect("cache parent"))
        .expect("mkdir cache parent");
    std::fs::write(&cache_path, "downloaded-artifact").expect("write cache artifact");

    let _env = EnvGuard::set_many(&[(EFFIGY_WORKSPACE_ARTIFACT_SOURCE_ENV, None)]);
    let resolved = resolve_local_effigy_repo_root_from_paths(&workspace, None, None);

    assert_eq!(resolved, Some(local_effigy));
}

#[test]
fn workspace_linux_cache_path_is_versioned() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-workspace-cache-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_home).expect("mkdir temp home");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let cache_path =
        linux_workspace_effigy_cache_path(LinuxWorkspaceTarget::X86_64Gnu).expect("cache path");

    assert!(cache_path.starts_with(&temp_home));
    assert!(cache_path
        .display()
        .to_string()
        .contains(&format!("v{}", env!("CARGO_PKG_VERSION"))));
    assert!(cache_path
        .display()
        .to_string()
        .ends_with("effigy-x86_64-unknown-linux-gnu"));
}

#[test]
fn linux_workspace_target_maps_machine_names() {
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("x86_64"),
        Some(LinuxWorkspaceTarget::X86_64Gnu)
    );
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("amd64"),
        Some(LinuxWorkspaceTarget::X86_64Gnu)
    );
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("aarch64"),
        Some(LinuxWorkspaceTarget::Aarch64Gnu)
    );
    assert_eq!(
        LinuxWorkspaceTarget::from_machine("arm64"),
        Some(LinuxWorkspaceTarget::Aarch64Gnu)
    );
    assert_eq!(LinuxWorkspaceTarget::from_machine("ppc64le"), None);
}

#[test]
fn sibling_effigy_repo_root_prefers_adjacent_effigy_checkout() {
    let parent = std::env::temp_dir().join(format!(
        "effigy-sibling-root-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = parent.join("underlay-reference");
    let effigy = parent.join("effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(effigy.join("tasks")).expect("mkdir tasks");
    std::fs::create_dir_all(effigy.join("containers")).expect("mkdir containers");
    std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
    std::fs::write(effigy.join("containers/effigy.containers.toml"), "").expect("write containers");

    let discovered = sibling_effigy_repo_root(&workspace).expect("discover sibling effigy");
    assert_eq!(discovered, effigy);
}

#[test]
fn sibling_effigy_repo_root_discovers_projects_effigy_from_ancestor() {
    let root = std::env::temp_dir().join(format!(
        "effigy-projects-sibling-root-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("legacy/sites/contactpatch");
    let effigy = root.join("projects/effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    std::fs::create_dir_all(effigy.join("tasks")).expect("mkdir tasks");
    std::fs::create_dir_all(effigy.join("containers")).expect("mkdir containers");
    std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
    std::fs::write(effigy.join("containers/effigy.containers.toml"), "").expect("write containers");

    let discovered = sibling_effigy_repo_root(&workspace).expect("discover sibling effigy");
    assert_eq!(discovered, effigy);
}

#[test]
fn configured_effigy_repo_root_reads_host_pointer_file() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-configured-source-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let effigy = temp_home.join("projects/effigy");
    std::fs::create_dir_all(effigy.join("tasks")).expect("mkdir tasks");
    std::fs::create_dir_all(effigy.join("containers")).expect("mkdir containers");
    std::fs::write(effigy.join("effigy.toml"), "").expect("write manifest");
    std::fs::write(effigy.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
    std::fs::write(effigy.join("containers/effigy.containers.toml"), "").expect("write containers");
    std::fs::create_dir_all(temp_home.join(".effigy")).expect("mkdir .effigy");
    std::fs::write(
        temp_home.join(".effigy/source.toml"),
        format!("repo_root = \"{}\"\n", effigy.display()),
    )
    .expect("write source config");

    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let discovered = configured_effigy_repo_root().expect("configured repo root");

    assert_eq!(discovered, effigy);
}

#[test]
fn resolve_local_effigy_repo_root_prefers_live_sibling_checkout_over_configured_pointer() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-source-priority-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(temp_home.join(".effigy")).expect("mkdir .effigy");
    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    let root = std::env::temp_dir().join(format!(
        "effigy-source-priority-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let workspace = root.join("underlay-reference");
    let sibling_effigy = root.join("effigy");
    let configured_effigy = temp_home.join("old-effigy");
    std::fs::create_dir_all(&workspace).expect("mkdir workspace");
    for repo in [&sibling_effigy, &configured_effigy] {
        std::fs::create_dir_all(repo.join("tasks")).expect("mkdir tasks");
        std::fs::create_dir_all(repo.join("containers")).expect("mkdir containers");
        std::fs::write(repo.join("effigy.toml"), "").expect("write manifest");
        std::fs::write(repo.join("tasks/effigy.tasks.toml"), "").expect("write tasks");
        std::fs::write(repo.join("containers/effigy.containers.toml"), "")
            .expect("write containers");
    }
    std::fs::write(
        temp_home.join(".effigy/source.toml"),
        format!("repo_root = \"{}\"\n", configured_effigy.display()),
    )
    .expect("write source config");

    let resolved = resolve_local_effigy_repo_root_from_paths(&workspace, None, None);

    assert_eq!(resolved, Some(sibling_effigy));
}

#[test]
fn persist_effigy_source_repo_root_writes_host_pointer_file() {
    let temp_home = std::env::temp_dir().join(format!(
        "effigy-persisted-source-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let effigy = temp_home.join("projects/effigy");
    std::fs::create_dir_all(&effigy).expect("mkdir effigy");

    let _home = EnvGuard::set_many(&[("HOME", Some(temp_home.display().to_string()))]);

    persist_effigy_source_repo_root(&effigy).expect("persist source config");
    let raw =
        std::fs::read_to_string(temp_home.join(".effigy/source.toml")).expect("read source config");

    assert!(raw.contains(&format!("repo_root = \"{}\"", effigy.display())));
}

#[test]
fn plain_workspace_session_shuts_down_only_if_it_started_the_system() {
    assert!(should_shutdown_started_system(
        false,
        WorkspaceGatewayState {
            routes_were_ready_before_handoff: true,
        },
        WorkspaceSessionOwnership::OwnStartedSystem,
        true,
    ));
    assert!(!should_shutdown_started_system(
        true,
        WorkspaceGatewayState {
            routes_were_ready_before_handoff: true,
        },
        WorkspaceSessionOwnership::OwnStartedSystem,
        true,
    ));
}

#[test]
fn seeded_workspace_session_shuts_down_after_successful_handoff() {
    assert!(should_shutdown_started_system(
        false,
        WorkspaceGatewayState {
            routes_were_ready_before_handoff: true,
        },
        WorkspaceSessionOwnership::LeaveSystemRunning,
        true,
    ));
}

#[test]
fn seeded_workspace_session_leaves_started_system_running_after_failed_handoff() {
    assert!(!should_shutdown_started_system(
        false,
        WorkspaceGatewayState {
            routes_were_ready_before_handoff: true,
        },
        WorkspaceSessionOwnership::LeaveSystemRunning,
        false,
    ));
    assert!(!should_shutdown_started_system(
        true,
        WorkspaceGatewayState {
            routes_were_ready_before_handoff: true,
        },
        WorkspaceSessionOwnership::LeaveSystemRunning,
        true,
    ));
}

#[test]
fn plain_workspace_session_shuts_down_adopted_stack_when_handoff_completed_gateway_readiness() {
    assert!(should_shutdown_started_system(
        true,
        WorkspaceGatewayState {
            routes_were_ready_before_handoff: false,
        },
        WorkspaceSessionOwnership::OwnStartedSystem,
        true,
    ));
}

#[test]
fn effective_workspace_repo_override_falls_back_to_repo_root() {
    let repo_root = Path::new("/tmp/demo-repo");
    assert_eq!(
        effective_workspace_repo_override(repo_root, None),
        Some(repo_root.to_path_buf())
    );
    assert_eq!(
        effective_workspace_repo_override(repo_root, Some(PathBuf::from("/tmp/explicit"))),
        Some(PathBuf::from("/tmp/explicit"))
    );
}

#[test]
fn workspace_effigy_install_command_targets_usr_local_bin() {
    let rendered = render_workspace_effigy_install_command("/tmp/effigy-host-1");
    assert!(rendered.contains("/tmp/effigy-host"));
    assert!(rendered.contains("/usr/local/bin/effigy"));
    assert!(rendered.contains("install -m 0755"));
}

#[test]
fn workspace_effigy_staging_path_is_unique() {
    let first = render_workspace_effigy_staging_path();
    let second = render_workspace_effigy_staging_path();
    assert_ne!(first, second);
    assert!(first.starts_with("/tmp/effigy-host-"));
    assert!(second.starts_with("/tmp/effigy-host-"));
}
