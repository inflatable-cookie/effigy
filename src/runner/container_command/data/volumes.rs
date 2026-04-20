use std::collections::BTreeSet;
use std::path::Path;

use effigy_catalog::volumes::{
    export_volume_command, import_volume_command, inspect_volume_command, list_volumes_command,
    merge_runtime_volume_metadata, parse_inspect_volume_metadata, parse_listed_volume_names,
    ManagedVolume,
};
use effigy_containers::{ContainerDataTransferAction, EffectiveContainerPolicy};

use super::super::support::run_runtime_volume_capture;
use super::RunnerError;

pub(super) fn run_volume_transfer(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
    archive_path: &Path,
    action: ContainerDataTransferAction,
) -> Result<(), RunnerError> {
    let command = match action {
        ContainerDataTransferAction::Export => export_volume_command(volume_name, archive_path),
        ContainerDataTransferAction::Import => import_volume_command(volume_name, archive_path),
    };
    run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    Ok(())
}

pub(super) fn hydrate_managed_volumes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    colima_running: bool,
) -> Result<Vec<ManagedVolume>, RunnerError> {
    let mut volumes = policy.managed_volumes.clone();
    volumes.sort_by(|left, right| left.name.cmp(&right.name));
    if !colima_running || volumes.is_empty() {
        return Ok(volumes);
    }

    let listed = run_runtime_volume_capture(
        repo_root,
        &policy.profile,
        &list_volumes_command(&policy.project_name),
    )?;
    let listed_names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .collect::<BTreeSet<_>>();

    let mut runtime = Vec::new();
    for volume in &volumes {
        if !listed_names.contains(&volume.name) {
            continue;
        }
        let output = run_runtime_volume_capture(
            repo_root,
            &policy.profile,
            &inspect_volume_command(&volume.name),
        )?;
        if let Some(metadata) =
            parse_inspect_volume_metadata(String::from_utf8_lossy(&output.stdout).as_ref())
        {
            runtime.push(metadata);
        }
    }

    Ok(merge_runtime_volume_metadata(&volumes, &runtime))
}

pub(super) fn resolve_managed_volume(
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
) -> Result<ManagedVolume, RunnerError> {
    let Some(volume) = policy
        .managed_volumes
        .iter()
        .find(|volume| volume.name == volume_name)
        .cloned()
    else {
        let available = policy
            .managed_volumes
            .iter()
            .map(|volume| format!("`{}`", volume.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(RunnerError::task_invocation(format!(
            "managed volume `{volume_name}` is not owned by container `{}` (available: {available})",
            policy.name
        )));
    };
    Ok(volume)
}

pub(super) fn validate_transfer_path(
    archive_path: &Path,
    action: ContainerDataTransferAction,
) -> Result<(), RunnerError> {
    match action {
        ContainerDataTransferAction::Export => {
            let parent = archive_path.parent().unwrap_or(Path::new("."));
            if !parent.is_dir() {
                return Err(RunnerError::task_invocation(format!(
                    "export path parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        ContainerDataTransferAction::Import => {
            if !archive_path.is_file() {
                return Err(RunnerError::task_invocation(format!(
                    "import archive not found: {}",
                    archive_path.display()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{hydrate_managed_volumes, resolve_managed_volume};
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::path::{Path, PathBuf};

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "default".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            shared_services: Vec::<SharedServiceBinding>::new(),
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            declared_ports: vec!["8080:80".to_owned()],
            ports_declared_explicitly: true,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            ui_tabs: vec![],
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }

    #[test]
    fn hydrate_managed_volumes_skips_runtime_metadata_when_colima_is_stopped() {
        let policy = EffectiveContainerPolicy {
            managed_volumes: vec![effigy_catalog::volumes::ManagedVolume {
                name: "demo-web-dev-db-data".to_owned(),
                service: "db".to_owned(),
                persist: true,
                size_bytes: None,
                mount_point: None,
            }],
            ..test_policy()
        };

        let hydrated = hydrate_managed_volumes(Path::new("/tmp"), &policy, false).expect("hydrate");
        assert_eq!(hydrated.len(), 1);
        assert!(hydrated[0].size_bytes.is_none());
        assert!(hydrated[0].mount_point.is_none());
    }

    #[test]
    fn resolve_managed_volume_rejects_unowned_name() {
        let policy = EffectiveContainerPolicy {
            managed_volumes: vec![effigy_catalog::volumes::ManagedVolume {
                name: "demo-web-dev-db-data".to_owned(),
                service: "db".to_owned(),
                persist: true,
                size_bytes: None,
                mount_point: None,
            }],
            ..test_policy()
        };

        let error = resolve_managed_volume(&policy, "other-volume").expect_err("should fail");
        assert!(error
            .to_string()
            .contains("managed volume `other-volume` is not owned by container `web`"));
    }
}
