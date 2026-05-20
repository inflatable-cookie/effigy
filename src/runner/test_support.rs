use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup,
};
use std::path::PathBuf;

pub(in crate::runner) fn effective_container_policy(
    name: &str,
    project_name: &str,
    primary_service: &str,
    compose_file: impl Into<PathBuf>,
) -> EffectiveContainerPolicy {
    let compose_file = compose_file.into();
    EffectiveContainerPolicy {
        name: name.to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files: vec![compose_file],
        compose_file_display: "docker-compose.yml".to_owned(),
        managed_volumes: vec![],
        shared_services: vec![],
        project_name: project_name.to_owned(),
        primary_service: primary_service.to_owned(),
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
        secret_delivery: effigy_manifest::ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: Vec::new(),
    }
}
