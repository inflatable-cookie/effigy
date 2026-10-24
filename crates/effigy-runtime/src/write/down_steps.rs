//! The live steps one `container down` shows.
//!
//! A teardown is one service at a time: each one is stopped and removed. The
//! steps name the service being stopped while the runtime stops it, then say
//! whether it is gone, so a person watching a slow stop knows which service is
//! holding it up (effigy#313).
//!
//! The steps draw only on a terminal: `effigy_ui::steps` decides that, and
//! `--json`, a pipe or CI reads nothing from here.

use std::path::Path;
use std::time::Instant;

use effigy_containers::exec::list_compose_containers_for_policy_including_stopped;
use effigy_containers::EffectiveContainerPolicy;
use effigy_ui::steps::{step, Step};

/// One service's teardown step.
#[derive(Debug)]
struct ServiceStep {
    service: String,
    container: String,
    step: Step,
    started: Instant,
}

/// The per-service teardown steps of one environment.
#[derive(Debug, Default)]
pub(super) struct DownSteps {
    steps: Vec<ServiceStep>,
}

impl DownSteps {
    /// The steps for every container this environment currently has, running
    /// or stopped. An inventory that cannot be read draws no steps: a
    /// teardown still runs, and its report says what happened.
    pub(super) fn observe(repo_root: &Path, policy: &EffectiveContainerPolicy) -> Self {
        let mut steps = Vec::new();
        if let Ok(rows) = list_compose_containers_for_policy_including_stopped(repo_root, policy) {
            for row in rows.into_iter().filter(|row| !row.oneoff) {
                let service = row
                    .service
                    .clone()
                    .unwrap_or_else(|| row.container_name.clone());
                let step = step(&format!("stopping {service}"));
                steps.push(ServiceStep {
                    service,
                    container: row.container_name,
                    step,
                    started: Instant::now(),
                });
            }
        }
        Self { steps }
    }

    /// How many services this teardown is moving.
    #[cfg(test)]
    pub(super) fn touched(&self) -> usize {
        self.steps.len()
    }

    /// End every step with what the runtime now reports: a service is either
    /// gone or it is not, and the second is a failure a person must see.
    pub(super) fn finish(&self, repo_root: &Path, policy: &EffectiveContainerPolicy) {
        let remaining = list_compose_containers_for_policy_including_stopped(repo_root, policy)
            .map(|rows| {
                rows.into_iter()
                    .filter(|row| !row.oneoff)
                    .map(|row| row.container_name)
                    .collect::<Vec<_>>()
            })
            .ok();
        for service in &self.steps {
            let duration = effigy_ui::human::human_duration(service.started.elapsed());
            match remaining.as_ref() {
                Some(remaining) if remaining.contains(&service.container) => service.step.fail(&format!(
                    "{} is still present after the teardown ({duration}); inspect it with `effigy container status` and `effigy container logs {}`",
                    service.service, service.service
                )),
                Some(_) => service.step.succeed(&format!(
                    "stopped and removed {} ({duration})",
                    service.service
                )),
                // The verification read failed, so the step keeps the runtime's
                // own word rather than claiming what it could not see.
                None => service.step.succeed(&format!(
                    "asked the runtime to stop and remove {} ({duration}); `effigy container status` confirms it",
                    service.service
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_inventory_draws_no_steps_instead_of_inventing_services() {
        // A path that is not a checkout: the inventory cannot be read.
        let policy = test_policy();
        let steps = DownSteps::observe(Path::new("/effigy-does-not-exist/down-steps"), &policy);
        assert_eq!(steps.touched(), 0);
    }

    fn test_policy() -> EffectiveContainerPolicy {
        use effigy_containers::{EffectiveComposeSource, SharedServiceBinding};
        use effigy_manifest::{
            ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
            ManifestContainerStartup,
        };
        use std::path::PathBuf;
        EffectiveContainerPolicy {
            render: None,
            repo_root: PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            compose_file_display: "docker-compose.yml".to_owned(),
            managed_volumes: vec![],
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services: Vec::<SharedServiceBinding>::new(),
            project_name: "demo-web-dev".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
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
}
