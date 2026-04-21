use std::path::Path;

use effigy_containers::{
    data_list_report, data_pull_production_report, data_transfer_report,
    exec::{colima_is_running, colima_profile_warnings},
    load_container_policy, validate_container_policy, ContainerDataHookResult,
    ContainerDataTransferAction, ContainerDataVolumeEntry, EffectiveComposeSource,
    EffectiveContainerPolicy,
};

use super::gateway_registration::register_gateway_routes_for_container;
use super::support::{
    annotate_registered_gateway_routes, annotate_shared_service_notes, annotate_warning_lines,
    ensure_shared_services_running, wait_for_container_ready,
};
use super::{render_container_report, RunnerError};

#[path = "data/hooks.rs"]
mod hooks;
#[path = "data/volumes.rs"]
mod volumes;

pub(super) fn run_container_data_list(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    ensure_generated_data_path(&policy, "list")?;

    let colima_running = colima_is_running(&policy, repo_root)?;
    let volumes = volumes::hydrate_managed_volumes(repo_root, &policy, colima_running)?
        .into_iter()
        .map(|volume| ContainerDataVolumeEntry {
            name: volume.name,
            service: volume.service,
            persist: volume.persist,
            size_bytes: volume.size_bytes,
            mount_point: volume.mount_point,
        })
        .collect::<Vec<_>>();

    Ok(render_container_report(
        data_list_report(&policy, colima_running, &volumes),
        output_json,
    ))
}

pub(super) fn run_container_data_export(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_container_data_transfer(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        ContainerDataTransferAction::Export,
    )
}

pub(super) fn run_container_data_import(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_container_data_transfer(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        ContainerDataTransferAction::Import,
    )
}

pub(super) fn run_container_data_pull_production(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    ensure_generated_data_path(&policy, "pull-production")?;
    let hook = policy.pull_production_hook.clone().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "container `{}` does not declare `[containers.{}.data].pull_production`",
            policy.name, policy.name
        ))
    })?;

    let colima_started = effigy_containers::exec::ensure_colima_running(&policy, repo_root)?;
    let shared_service_notes = ensure_shared_services_running(&policy)?;
    super::signals::run_docker_capture(
        repo_root,
        &policy,
        &effigy_containers::compose::compose_up_args(&policy),
        "docker compose up",
    )?;
    let health = wait_for_container_ready(&policy, None)?;
    let gateway_routes = register_gateway_routes_for_container(repo_root, &policy)?;
    hooks::execute_pull_production_hook(repo_root, &policy, &hook)?;

    let mut report = data_pull_production_report(
        &policy,
        &ContainerDataHookResult { hook },
        colima_started,
        health,
    );
    annotate_shared_service_notes(&mut report, &shared_service_notes);
    annotate_registered_gateway_routes(&mut report, &gateway_routes);
    annotate_warning_lines(&mut report, &colima_profile_warnings(&policy, repo_root));
    Ok(render_container_report(report, output_json))
}

fn ensure_generated_data_path(
    policy: &EffectiveContainerPolicy,
    action: &str,
) -> Result<(), RunnerError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data {action}` is only supported on the generated-compose path in this batch",
            policy.name
        )));
    }
    Ok(())
}

fn run_container_data_transfer(
    repo_root: &Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &Path,
    output_json: bool,
    action: ContainerDataTransferAction,
) -> Result<String, RunnerError> {
    let policy = load_container_policy(repo_root, name)?;
    validate_container_policy(repo_root, &policy)?;
    ensure_generated_data_path(
        &policy,
        match action {
            ContainerDataTransferAction::Export => "export",
            ContainerDataTransferAction::Import => "import",
        },
    )?;
    volumes::validate_transfer_path(archive_path, action)?;
    if !colima_is_running(&policy, repo_root)? {
        return Err(RunnerError::task_invocation(format!(
            "Colima profile `{}` is not running for container `{}`",
            policy.profile, policy.name
        )));
    }

    let volume = volumes::resolve_managed_volume(&policy, volume_name)?;
    volumes::run_volume_transfer(repo_root, &policy, &volume.name, archive_path, action)?;
    let report = data_transfer_report(
        &policy,
        action,
        &ContainerDataVolumeEntry {
            name: volume.name,
            service: volume.service,
            persist: volume.persist,
            size_bytes: volume.size_bytes,
            mount_point: volume.mount_point,
        },
        archive_path,
    );
    Ok(render_container_report(report, output_json))
}

#[cfg(test)]
mod tests {
    use super::{
        run_container_data_export, run_container_data_import, run_container_data_list,
        run_container_data_pull_production,
    };
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-data-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    fn test_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
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
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }

    #[test]
    fn run_container_data_list_rejects_direct_compose_ownership() {
        let root = temp_repo("data-list-direct");
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

        let error = run_container_data_list(&root, None, false).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data list` is only supported on the generated-compose path"));
    }

    #[test]
    fn run_container_data_export_rejects_direct_compose_ownership() {
        let root = temp_repo("data-export-direct");
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

        let error = run_container_data_export(
            &root,
            None,
            "demo-web-dev-db-data",
            Path::new("/tmp/demo.tar.gz"),
            false,
        )
        .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data export` is only supported on the generated-compose path"));
    }

    #[test]
    fn run_container_data_import_rejects_direct_compose_ownership() {
        let root = temp_repo("data-import-direct");
        let archive = root.join("backup.tar.gz");
        fs::write(&archive, "fake archive").expect("write archive");
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

        let error = run_container_data_import(&root, None, "demo-web-dev-db-data", &archive, false)
            .expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data import` is only supported on the generated-compose path"));
    }

    #[test]
    fn run_container_data_pull_production_rejects_direct_compose_ownership() {
        let root = temp_repo("data-pull-production-direct");
        fs::write(
            root.join("effigy.toml"),
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.data]
pull_production = "scripts/pull-production.sh"
"#,
        )
        .expect("write manifest");
        fs::create_dir_all(root.join("infra/dev")).expect("mkdir compose dir");
        fs::write(root.join("infra/dev/docker-compose.yml"), "services: {}\n").expect("compose");

        let error =
            run_container_data_pull_production(&root, None, false).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("`data pull-production` is only supported on the generated-compose path"));
    }

    #[test]
    fn test_policy_stays_constructible_for_data_tests() {
        let policy = test_policy();
        assert_eq!(policy.name, "web");
    }
}
