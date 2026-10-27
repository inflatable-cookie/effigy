use super::*;
use crate::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_manifest::{
    with_test_user_config_home, ManifestContainerDriver, ManifestContainerOnTaskExit,
    ManifestContainerShutdownMode, ManifestContainerStartup,
};
use std::cell::Cell;
use std::path::PathBuf;

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
    crate::test_env_lock()
}

/// Which resolver a row asks, and therefore which of its environment inputs
/// the row sets up.
#[derive(Clone, Copy)]
enum Resolves {
    /// `resolve_compose_backend`, from the environment and user config only.
    Environment,
    /// `resolve_compose_backend_for_policy`, which also sees the declared
    /// driver and the host CLI on `PATH`.
    Policy,
}

/// An explicit backend override, a declared Colima driver and a user-global
/// `containerd` preference each select the backend they name.
#[test]
fn compose_backend_resolution_cases() {
    let temp = tempfile::tempdir().expect("tempdir");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    std::fs::write(bin.join("docker"), "#!/bin/sh\n").expect("write docker");
    let home = temp.path().join(".effigy-home");
    std::fs::create_dir_all(&home).expect("mkdir home");

    type Row = (
        &'static str,
        Option<&'static str>,
        Option<&'static str>,
        Resolves,
        ComposeBackend,
    );
    let cases: [Row; 4] = [
        (
            "EFFIGY_COMPOSE_BACKEND=containerd",
            Some("containerd"),
            None,
            Resolves::Environment,
            ComposeBackend::ColimaNerdctl,
        ),
        (
            "EFFIGY_COMPOSE_BACKEND=docker",
            Some("docker"),
            None,
            Resolves::Environment,
            ComposeBackend::Docker,
        ),
        (
            "the policy declares the Colima driver",
            None,
            None,
            Resolves::Policy,
            ComposeBackend::ColimaNerdctl,
        ),
        (
            "the user config prefers containerd",
            None,
            Some("[containers]\nbackend = \"containerd\"\n"),
            Resolves::Environment,
            ComposeBackend::ColimaNerdctl,
        ),
    ];
    for (label, backend_env, user_config, resolves, expected) in cases {
        let _lock = env_lock();
        let previous_path = std::env::var_os("PATH");
        let previous_backend = std::env::var_os("EFFIGY_COMPOSE_BACKEND");
        if let Some(config) = user_config {
            std::fs::write(home.join("config.toml"), config).expect("write config");
        }
        unsafe {
            std::env::set_var("PATH", bin.display().to_string());
            match backend_env {
                Some(value) => std::env::set_var("EFFIGY_COMPOSE_BACKEND", value),
                None => std::env::remove_var("EFFIGY_COMPOSE_BACKEND"),
            }
        }
        let backend = match resolves {
            Resolves::Environment => {
                if user_config.is_some() {
                    with_test_user_config_home(&home, resolve_compose_backend)
                } else {
                    resolve_compose_backend()
                }
            }
            Resolves::Policy => {
                resolve_compose_backend_for_policy(&test_policy(EffectiveComposeSource::Direct))
            }
        };
        unsafe {
            match previous_path {
                Some(value) => std::env::set_var("PATH", value),
                None => std::env::remove_var("PATH"),
            }
            match previous_backend {
                Some(value) => std::env::set_var("EFFIGY_COMPOSE_BACKEND", value),
                None => std::env::remove_var("EFFIGY_COMPOSE_BACKEND"),
            }
        }
        assert_eq!(backend, expected, "{label}");
    }
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

fn test_policy(compose_source: EffectiveComposeSource) -> EffectiveContainerPolicy {
    EffectiveContainerPolicy {
        render: None,
        repo_root: std::path::PathBuf::from("/tmp"),
        name: "web".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source,
        compose_files: vec![PathBuf::from("docker-compose.yml")],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        borrowed_volumes: vec![],
        data_peers: Vec::new(),
        data_from: None,
        shared_services: vec![],
        project_name: "demo".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: None,
        dns_base_domain: None,
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
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: vec![],
    }
}

#[test]
fn compose_up_args_are_one_apply_for_both_sources() {
    for source in [
        EffectiveComposeSource::Generated,
        EffectiveComposeSource::Direct,
    ] {
        let args = compose_up_args(&test_policy(source));
        let rendered = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(rendered.windows(2).any(|window| window == ["up", "-d"]));
        assert!(rendered.contains(&"--remove-orphans".to_owned()));
        assert!(!rendered.contains(&"--force-recreate".to_owned()));
        assert!(!rendered.contains(&"--no-recreate".to_owned()));
    }
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
