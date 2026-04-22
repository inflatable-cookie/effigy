use super::*;
use crate::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};
use std::path::PathBuf;

#[test]
fn resolve_compose_backend_returns_something() {
    // Can't assert which backend without knowing the host, but it
    // should not panic.
    let _ = resolve_compose_backend();
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
