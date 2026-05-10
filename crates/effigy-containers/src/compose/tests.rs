use super::*;
use crate::{
    load_runtime_backend_override, write_runtime_backend_override, EffectiveComposeSource,
    EffectiveContainerPolicy,
};
use effigy_manifest::{
    with_test_user_config_home, ManifestContainerDriver, ManifestContainerOnTaskExit,
    ManifestContainerShutdownMode, ManifestContainerStartup,
};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

thread_local! {
    static TEST_COMPOSE_BACKEND_OVERRIDE: Cell<Option<ComposeBackend>> = const { Cell::new(None) };
}

pub(super) fn test_compose_backend_override() -> Option<ComposeBackend> {
    TEST_COMPOSE_BACKEND_OVERRIDE.with(Cell::get)
}

pub(super) fn with_test_compose_backend<T>(backend: ComposeBackend, run: impl FnOnce() -> T) -> T {
    TEST_COMPOSE_BACKEND_OVERRIDE.with(|slot| {
        let previous = slot.replace(Some(backend));
        let result = run();
        slot.set(previous);
        result
    })
}

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn resolve_compose_backend_returns_something() {
    // Can't assert which backend without knowing the host, but it
    // should not panic.
    let _ = resolve_compose_backend();
}

#[test]
fn resolve_compose_backend_honors_colima_override_env() {
    let _lock = env_lock();
    let previous = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
    unsafe {
        std::env::set_var("EFFIGY_COMPOSE_BACKEND", "containerd");
    }
    let backend = resolve_compose_backend();
    match previous {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
        },
        None => unsafe {
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        },
    }
    assert_eq!(backend, ComposeBackend::ColimaNerdctl);
}

#[test]
fn resolve_compose_backend_honors_docker_override_env() {
    let _lock = env_lock();
    let previous = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
    unsafe {
        std::env::set_var("EFFIGY_COMPOSE_BACKEND", "docker");
    }
    let backend = resolve_compose_backend();
    match previous {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
        },
        None => unsafe {
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        },
    }
    assert_eq!(backend, ComposeBackend::Docker);
}

#[test]
fn resolve_compose_backend_for_policy_prefers_declared_colima_driver() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    std::fs::write(bin.join("docker"), "#!/bin/sh\n").expect("write docker");
    let previous_path = std::env::var_os("PATH");
    let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
    unsafe {
        std::env::set_var("PATH", bin.display().to_string());
        std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
    }
    let backend = resolve_compose_backend_for_policy(&test_policy(EffectiveComposeSource::Direct));
    match previous_path {
        Some(value) => unsafe {
            std::env::set_var("PATH", value);
        },
        None => unsafe {
            std::env::remove_var("PATH");
        },
    }
    match previous_backend {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
        },
        None => unsafe {
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        },
    }
    assert_eq!(backend, ComposeBackend::ColimaNerdctl);
}

#[test]
fn resolve_compose_backend_honors_user_global_containerd_preference() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join(".effigy-home");
    std::fs::create_dir_all(&home).expect("mkdir home");
    std::fs::write(
        home.join("config.toml"),
        "[containers]\nbackend = \"containerd\"\n",
    )
    .expect("write config");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    std::fs::write(bin.join("docker"), "#!/bin/sh\n").expect("write docker");
    let previous_path = std::env::var_os("PATH");
    let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
    unsafe {
        std::env::set_var("PATH", bin.display().to_string());
        std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
    }
    let backend = with_test_user_config_home(&home, resolve_compose_backend);
    match previous_path {
        Some(value) => unsafe {
            std::env::set_var("PATH", value);
        },
        None => unsafe {
            std::env::remove_var("PATH");
        },
    }
    match previous_backend {
        Some(value) => unsafe {
            std::env::set_var("EFFIGY_COMPOSE_BACKEND", value);
        },
        None => unsafe {
            std::env::remove_var("EFFIGY_COMPOSE_BACKEND");
        },
    }
    assert_eq!(backend, ComposeBackend::ColimaNerdctl);
}

#[test]
fn scoped_runtime_backend_override_ignores_legacy_repo_wide_backend() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(repo_root.join(".effigy/runtime/compose")).expect("mkdir runtime dir");
    std::fs::write(
        repo_root.join(".effigy/runtime/compose/.effigy-runtime.json"),
        "backend = \"docker-compose\"\n",
    )
    .expect("write legacy metadata");

    assert_eq!(load_runtime_backend_override(&repo_root, Some("web")), None);
    assert_eq!(
        load_runtime_backend_override(&repo_root, None),
        Some(crate::BackendId::docker_compose())
    );
}

#[test]
fn write_runtime_backend_override_scopes_backend_to_container_name() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("mkdir repo");

    write_runtime_backend_override(
        &repo_root,
        Some("linux-release"),
        &crate::BackendId::colima_nerdctl(),
    )
    .expect("write scoped metadata");

    assert_eq!(
        load_runtime_backend_override(&repo_root, Some("linux-release")),
        Some(crate::BackendId::colima_nerdctl())
    );
    assert_eq!(load_runtime_backend_override(&repo_root, Some("web")), None);
}

#[test]
fn resolve_host_cli_program_prefers_path_hits() {
    let _lock = env_lock();
    let temp = tempfile::tempdir().expect("tempdir");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    std::fs::write(bin.join("colima"), "#!/bin/sh\n").expect("write colima");
    let previous = std::env::var_os("PATH");
    unsafe {
        std::env::set_var("PATH", bin.display().to_string());
    }
    let resolved = resolve_host_cli_program("colima");
    match previous {
        Some(value) => unsafe {
            std::env::set_var("PATH", value);
        },
        None => unsafe {
            std::env::remove_var("PATH");
        },
    }
    assert_eq!(resolved, bin.join("colima").into_os_string());
}

#[test]
fn shutdown_labels() {
    assert_eq!(
        shutdown_label(ManifestContainerShutdownMode::Graceful),
        "graceful"
    );
    assert_eq!(
        shutdown_label(ManifestContainerShutdownMode::Immediate),
        "immediate"
    );
}

#[test]
fn on_task_exit_labels() {
    assert_eq!(
        on_task_exit_label(ManifestContainerOnTaskExit::Stop),
        "stop"
    );
    assert_eq!(
        on_task_exit_label(ManifestContainerOnTaskExit::LeaveRunning),
        "leave-running"
    );
}

fn test_policy(compose_source: EffectiveComposeSource) -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        name: "web".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source,
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        shared_services: vec![],
        project_name: "demo".to_owned(),
        primary_service: "app".to_owned(),
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
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: vec![],
    }
}

#[test]
fn compose_up_args_force_recreate_generated_compose() {
    let args = compose_up_args(&test_policy(EffectiveComposeSource::Generated));
    let rendered = args
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(rendered.windows(2).any(|window| window == ["up", "-d"]));
    assert!(rendered.contains(&"--build".to_owned()));
    assert!(rendered.contains(&"--force-recreate".to_owned()));
}

#[test]
fn compose_up_args_leave_direct_compose_without_force_recreate() {
    let args = compose_up_args(&test_policy(EffectiveComposeSource::Direct));
    let rendered = args
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(rendered.contains(&"--build".to_owned()));
    assert!(!rendered.contains(&"--force-recreate".to_owned()));
}

#[test]
fn normalize_compose_command_args_wraps_bare_exec_tail() {
    let args = normalize_compose_command_args(
        &test_policy(EffectiveComposeSource::Direct),
        &[
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from("app"),
        ],
    );
    let rendered = args
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        rendered,
        vec![
            "compose".to_owned(),
            "-f".to_owned(),
            "docker-compose.yml".to_owned(),
            "-p".to_owned(),
            "demo".to_owned(),
            "exec".to_owned(),
            "-T".to_owned(),
            "app".to_owned(),
        ]
    );
}
