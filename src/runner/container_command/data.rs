use effigy_runtime::data::{
    run_container_data_pull_production as run_runtime_container_data_pull_production,
    RegisteredGatewayRoute,
};
use std::path::Path;

use super::gateway_registration::register_gateway_routes_for_container;
use super::runtime_error_from_runner;
use super::support::{ensure_shared_services_running, wait_for_container_ready};
use super::RunnerError;

#[path = "data/hooks.rs"]
mod hooks;

pub(super) fn run_container_data_pull_production(
    repo_root: &Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_runtime_container_data_pull_production(
        repo_root,
        name,
        output_json,
        |policy| ensure_shared_services_running(policy).map_err(runtime_error_from_runner),
        |policy| wait_for_container_ready(policy, None).map_err(runtime_error_from_runner),
        |repo_root, policy| {
            register_gateway_routes_for_container(repo_root, policy)
                .map(|routes| {
                    routes
                        .into_iter()
                        .map(|route| RegisteredGatewayRoute {
                            domain: route.domain,
                            target: route.target,
                            dns_ip: route.dns_ip,
                            tls: route.tls,
                        })
                        .collect()
                })
                .map_err(runtime_error_from_runner)
        },
        |repo_root, policy, hook| {
            hooks::execute_pull_production_hook(repo_root, policy, hook)
                .map_err(runtime_error_from_runner)
        },
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::run_container_data_pull_production;
    use effigy_containers::{
        EffectiveComposeSource, EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
        ManifestContainerStartup,
    };
    use effigy_runtime::data::{
        run_container_data_export, run_container_data_import, run_container_data_list,
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
            service_aliases: vec![],
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

        let error = run_container_data_list(&root, None, false, |_, _, _| unreachable!())
            .expect_err("should fail");
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
            |_, _, _| unreachable!(),
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

        let error = run_container_data_import(
            &root,
            None,
            "demo-web-dev-db-data",
            &archive,
            false,
            |_, _, _| unreachable!(),
        )
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
