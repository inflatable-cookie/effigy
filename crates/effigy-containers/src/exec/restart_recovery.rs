//! Recover owned Compose services that stay Exited after a Colima VM restart.
//!
//! `nerdctl compose up` can return success while owned containers remain
//! stopped. Manual `nerdctl start` brings them back and may log a stale
//! health-check timer unit. Effigy starts only containers whose Compose
//! project label matches an owned project, does not recreate them (volumes
//! stay), and does not delete systemd units.

use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use super::colima_runtime::run_runtime_command_capture_for_policy_allow_failure_with_timeout;
use super::implementation::{
    list_compose_containers_for_project_including_stopped, ContainerExecError,
};
use super::parse::{
    compose_status_is_running, compose_status_needs_start, looks_like_stale_healthcheck_timer,
    RunningComposeContainer,
};
use crate::compose::{resolve_compose_backend_for_repo, ComposeBackend};
use crate::EffectiveContainerPolicy;

const OWNED_SERVICE_START_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnedServiceStartRecovery {
    pub started: Vec<String>,
    pub warnings: Vec<String>,
}

impl OwnedServiceStartRecovery {
    fn merge(&mut self, other: Self) {
        self.started.extend(other.started);
        self.warnings.extend(other.warnings);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStartResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn recover_exited_owned_compose_services(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<OwnedServiceStartRecovery, ContainerExecError> {
    let mut recovery =
        recover_exited_owned_compose_services_for_project(repo_root, policy, &policy.project_name)?;
    for shared in &policy.shared_services {
        recovery.merge(recover_exited_owned_compose_services_for_project(
            repo_root,
            policy,
            &shared.project_name,
        )?);
    }
    Ok(recovery)
}

pub fn recover_exited_owned_compose_services_for_project(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Result<OwnedServiceStartRecovery, ContainerExecError> {
    let backend = resolve_compose_backend_for_repo(repo_root, policy);
    recover_exited_owned_services_with(
        project_name,
        &policy.profile,
        backend,
        || list_compose_containers_for_project_including_stopped(repo_root, policy, project_name),
        |container_name| {
            let output = run_runtime_command_capture_for_policy_allow_failure_with_timeout(
                repo_root,
                policy,
                &[OsString::from("start"), OsString::from(container_name)],
                &format!("start owned container `{container_name}`"),
                OWNED_SERVICE_START_TIMEOUT,
            )?;
            Ok(RuntimeStartResult {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        },
    )
}

pub fn recover_exited_owned_services_with(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    mut inspect: impl FnMut() -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
    mut start: impl FnMut(&str) -> Result<RuntimeStartResult, ContainerExecError>,
) -> Result<OwnedServiceStartRecovery, ContainerExecError> {
    let before = inspect()?;
    let targets = owned_services_needing_start(project_name, &before);
    if targets.is_empty() {
        return Ok(OwnedServiceStartRecovery::default());
    }

    let mut recovery = OwnedServiceStartRecovery::default();
    let mut start_errors = Vec::new();
    for row in &targets {
        match start(&row.container_name) {
            Ok(result) => {
                recovery.started.push(row.container_name.clone());
                if looks_like_stale_healthcheck_timer(&result.stdout, &result.stderr) {
                    recovery.warnings.push(stale_timer_warning(
                        row,
                        &result.stderr,
                        result.success,
                    ));
                }
                if !result.success {
                    start_errors.push((row.clone(), result));
                }
            }
            Err(error) => {
                start_errors.push((
                    row.clone(),
                    RuntimeStartResult {
                        success: false,
                        stdout: String::new(),
                        stderr: error.to_string(),
                    },
                ));
            }
        }
    }

    let after = match inspect() {
        Ok(rows) => rows,
        Err(error) => {
            return Err(failure_from_last_observed_state(
                project_name,
                profile,
                backend,
                &targets,
                &before,
                &start_errors,
                error,
            ));
        }
    };
    if let Some(error) = persistent_start_failure(
        project_name,
        profile,
        backend,
        &targets,
        &after,
        &start_errors,
    ) {
        return Err(error);
    }
    Ok(recovery)
}

fn owned_services_needing_start(
    project_name: &str,
    rows: &[RunningComposeContainer],
) -> Vec<RunningComposeContainer> {
    rows.iter()
        .filter(|row| row.project_name.as_deref() == Some(project_name))
        .filter(|row| compose_status_needs_start(&row.status))
        .cloned()
        .collect()
}

fn persistent_start_failure(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    targets: &[RunningComposeContainer],
    after: &[RunningComposeContainer],
    start_errors: &[(RunningComposeContainer, RuntimeStartResult)],
) -> Option<ContainerExecError> {
    for target in targets {
        let current = after.iter().find(|row| {
            row.container_name == target.container_name
                && row.project_name.as_deref() == Some(project_name)
        });
        let still_down = current
            .map(|row| !compose_status_is_running(&row.status))
            .unwrap_or(true);
        if !still_down {
            continue;
        }
        let status = current
            .map(|row| row.status.as_str())
            .unwrap_or(target.status.as_str());
        let backend_text = start_errors
            .iter()
            .find(|(row, _)| row.container_name == target.container_name)
            .map(|(_, result)| format!("{}\n{}", result.stdout.trim(), result.stderr.trim()))
            .unwrap_or_default();
        return Some(persistent_exited_service_error(
            project_name,
            profile,
            backend,
            target,
            status,
            backend_text.trim(),
        ));
    }
    None
}

fn failure_from_last_observed_state(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    targets: &[RunningComposeContainer],
    before: &[RunningComposeContainer],
    start_errors: &[(RunningComposeContainer, RuntimeStartResult)],
    inspect_error: ContainerExecError,
) -> ContainerExecError {
    let inspect_text = inspect_error.to_string();
    let mut start_errors = start_errors.to_vec();
    for target in targets {
        match start_errors
            .iter_mut()
            .find(|(row, _)| row.container_name == target.container_name)
        {
            Some((_, result)) => {
                if !result.stderr.is_empty() {
                    result.stderr.push('\n');
                }
                result.stderr.push_str(&inspect_text);
            }
            None => start_errors.push((
                target.clone(),
                RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: inspect_text.clone(),
                },
            )),
        }
    }
    persistent_start_failure(
        project_name,
        profile,
        backend,
        targets,
        before,
        &start_errors,
    )
    .unwrap_or(inspect_error)
}

fn persistent_exited_service_error(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    row: &RunningComposeContainer,
    status: &str,
    backend_text: &str,
) -> ContainerExecError {
    let service = row
        .service
        .as_deref()
        .unwrap_or(row.container_name.as_str());
    let recovery_command = owned_start_recovery_command(backend, profile, &row.container_name);
    let stale = looks_like_stale_healthcheck_timer("", backend_text);
    let mut stderr = format!(
        "service `{service}` in project `{project_name}` is `{status}` after compose up and a start of owned container `{}`.\nnext: `{recovery_command}`\nEffigy did not recreate the container or delete systemd units, so persistent volumes including Postgres crash-recovery data stay.",
        row.container_name
    );
    if stale {
        stderr.push_str(
            "\nbackend reported a stale nerdctl health-check timer (`Unit <id>.timer was already loaded or has a fragment file`). That is a nerdctl systemd-timer defect; Effigy will not delete timer units.",
        );
    }
    if !backend_text.is_empty() {
        stderr.push_str("\nbackend:\n");
        stderr.push_str(backend_text);
    }
    ContainerExecError::Failure {
        command: format!("start owned service `{service}`"),
        code: None,
        stdout: String::new(),
        stderr,
    }
}

fn stale_timer_warning(
    row: &RunningComposeContainer,
    stderr: &str,
    start_succeeded: bool,
) -> String {
    let service = row
        .service
        .as_deref()
        .unwrap_or(row.container_name.as_str());
    if start_succeeded {
        format!(
            "nerdctl reported a stale health-check timer while starting owned service `{service}` (`{}`); the container is running and systemd units were not deleted",
            excerpt_timer_line(stderr)
        )
    } else {
        format!(
            "nerdctl reported a stale health-check timer while starting owned service `{service}` (`{}`)",
            excerpt_timer_line(stderr)
        )
    }
}

fn excerpt_timer_line(stderr: &str) -> String {
    stderr
        .lines()
        .find(|line| {
            let lowered = line.to_ascii_lowercase();
            lowered.contains(".timer") || lowered.contains("already loaded")
        })
        .map(str::trim)
        .unwrap_or(stderr.trim())
        .to_owned()
}

fn owned_start_recovery_command(
    backend: ComposeBackend,
    profile: &str,
    container_name: &str,
) -> String {
    match backend {
        ComposeBackend::Docker => format!("docker start {container_name}"),
        ComposeBackend::ColimaNerdctl => {
            format!("colima nerdctl --profile {profile} -- start {container_name}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STALE_TIMER: &str = "time=\"2026-09-28T12:00:00Z\" level=warning msg=\"systemd-run --unit=dd94022f7dd0.timer: Unit dd94022f7dd0.timer was already loaded or has a fragment file\"";

    fn row(project: &str, service: &str, container: &str, status: &str) -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: container.to_owned(),
            status: status.to_owned(),
            ports: Vec::new(),
            project_name: Some(project.to_owned()),
            working_dir: Some("/tmp/acowtancy".to_owned()),
            service: Some(service.to_owned()),
        }
    }

    #[test]
    fn recoverable_exited_service_is_started_despite_stale_timer() {
        let inspect_states = [
            vec![
                row(
                    "acowtancy-shared-pg",
                    "postgres",
                    "acowtancy-postgres-1",
                    "Exited (255) 2 minutes ago",
                ),
                row(
                    "someone-else",
                    "db",
                    "foreign-db-1",
                    "Exited (0) 1 second ago",
                ),
            ],
            vec![row(
                "acowtancy-shared-pg",
                "postgres",
                "acowtancy-postgres-1",
                "Up 1 second",
            )],
        ];
        let mut inspect_calls = 0;
        let mut started = Vec::new();
        let recovery = recover_exited_owned_services_with(
            "acowtancy-shared-pg",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |container| {
                started.push(container.to_owned());
                Ok(RuntimeStartResult {
                    success: true,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
        )
        .expect("owned exited service should recover");

        assert_eq!(started, vec!["acowtancy-postgres-1".to_owned()]);
        assert_eq!(recovery.started, vec!["acowtancy-postgres-1".to_owned()]);
        assert_eq!(recovery.warnings.len(), 1);
        assert!(recovery.warnings[0].contains("stale health-check timer"));
        assert!(recovery.warnings[0].contains("postgres"));
        assert!(recovery.warnings[0].contains("systemd units were not deleted"));
    }

    #[test]
    fn persistent_exited_service_is_a_bounded_backend_failure() {
        let inspect_states = [
            vec![row(
                "acowtancy-shared-mysql",
                "mysql",
                "acowtancy-mysql-1",
                "Exited (255) 2 minutes ago",
            )],
            vec![row(
                "acowtancy-shared-mysql",
                "mysql",
                "acowtancy-mysql-1",
                "Exited (255) 2 minutes ago",
            )],
        ];
        let mut inspect_calls = 0;
        let error = recover_exited_owned_services_with(
            "acowtancy-shared-mysql",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |_| {
                Ok(RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
        )
        .expect_err("still-exited owned service must fail");

        let detail = error.to_string();
        assert!(detail.contains("service `mysql`"), "got: {detail}");
        assert!(detail.contains("Exited (255)"), "got: {detail}");
        assert!(
            detail.contains("colima nerdctl --profile effigy -- start acowtancy-mysql-1"),
            "got: {detail}"
        );
        assert!(detail.contains("did not recreate"), "got: {detail}");
        assert!(
            detail.contains("stale nerdctl health-check timer"),
            "got: {detail}"
        );
        assert!(
            detail.contains("will not delete timer units"),
            "got: {detail}"
        );
        assert!(!detail.contains("systemctl"), "got: {detail}");
        assert!(!detail.contains("rm -f"), "got: {detail}");
    }

    #[test]
    fn running_and_foreign_containers_are_left_alone() {
        let mut started = Vec::new();
        let recovery = recover_exited_owned_services_with(
            "demo-web",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            || {
                Ok(vec![
                    row("demo-web", "app", "demo-app-1", "Up 10 seconds"),
                    row(
                        "other-project",
                        "postgres",
                        "other-postgres-1",
                        "Exited (0) 1 second ago",
                    ),
                ])
            },
            |container| {
                started.push(container.to_owned());
                panic!("start must not run for running or foreign containers, got {container}");
            },
        )
        .expect("no owned exited services");

        assert!(started.is_empty());
        assert!(recovery.started.is_empty());
        assert!(recovery.warnings.is_empty());
    }

    #[test]
    fn recovery_starts_by_container_name_not_recreate() {
        let inspect_states = [
            vec![row("demo-web", "postgres", "demo-postgres-1", "Created")],
            vec![row(
                "demo-web",
                "postgres",
                "demo-postgres-1",
                "Up 1 second",
            )],
        ];
        let mut inspect_calls = 0;
        let mut started = Vec::new();
        recover_exited_owned_services_with(
            "demo-web",
            "effigy",
            ComposeBackend::Docker,
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |container| {
                started.push(container.to_owned());
                Ok(RuntimeStartResult {
                    success: true,
                    stdout: container.to_owned(),
                    stderr: String::new(),
                })
            },
        )
        .expect("created owned service should start");

        assert_eq!(started, vec!["demo-postgres-1".to_owned()]);
    }

    #[test]
    fn docker_persistent_failure_names_docker_start() {
        let inspect_states = [
            vec![row(
                "demo-web",
                "app",
                "demo-app-1",
                "Exited (1) 3 seconds ago",
            )],
            vec![row(
                "demo-web",
                "app",
                "demo-app-1",
                "Exited (1) 3 seconds ago",
            )],
        ];
        let mut inspect_calls = 0;
        let error = recover_exited_owned_services_with(
            "demo-web",
            "effigy",
            ComposeBackend::Docker,
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |_| {
                Ok(RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: "cannot start container".to_owned(),
                })
            },
        )
        .expect_err("persistent docker failure");

        let detail = error.to_string();
        assert!(detail.contains("docker start demo-app-1"), "got: {detail}");
        assert!(
            !detail.contains("stale nerdctl health-check timer"),
            "got: {detail}"
        );
    }

    #[test]
    fn unlabeled_rows_are_not_owned() {
        let recovery = recover_exited_owned_services_with(
            "demo-web",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            || {
                Ok(vec![RunningComposeContainer {
                    container_name: "mystery-1".to_owned(),
                    status: "Exited (0) 1 second ago".to_owned(),
                    ports: Vec::new(),
                    project_name: None,
                    working_dir: None,
                    service: None,
                }])
            },
            |container| panic!("unlabeled container {container} is not owned"),
        )
        .expect("unlabeled rows are skipped");
        assert!(recovery.started.is_empty());
    }

    #[test]
    fn start_timeout_reports_observed_exited_status() {
        let inspect_states = [
            vec![row(
                "acowtancy-shared-pg",
                "postgres",
                "acowtancy-postgres-1",
                "Exited (255) 2 minutes ago",
            )],
            vec![row(
                "acowtancy-shared-pg",
                "postgres",
                "acowtancy-postgres-1",
                "Exited (255) 2 minutes ago",
            )],
        ];
        let mut inspect_calls = 0;
        let error = recover_exited_owned_services_with(
            "acowtancy-shared-pg",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |_| {
                Err(ContainerExecError::Failure {
                    command: "start owned container `acowtancy-postgres-1`".to_owned(),
                    code: None,
                    stdout: String::new(),
                    stderr: "[effigy] command timed out after 30s".to_owned(),
                })
            },
        )
        .expect_err("timed-out start must fail with observed status");

        let detail = error.to_string();
        assert!(detail.contains("service `postgres`"), "got: {detail}");
        assert!(detail.contains("Exited (255)"), "got: {detail}");
        assert!(
            detail.contains("command timed out after 30s"),
            "got: {detail}"
        );
        assert!(
            detail.contains("colima nerdctl --profile effigy -- start acowtancy-postgres-1"),
            "got: {detail}"
        );
    }

    #[test]
    fn inspect_timeout_after_start_keeps_last_observed_status() {
        let mut inspect_calls = 0;
        let error = recover_exited_owned_services_with(
            "acowtancy-shared-pg",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            || {
                inspect_calls += 1;
                if inspect_calls == 1 {
                    Ok(vec![row(
                        "acowtancy-shared-pg",
                        "postgres",
                        "acowtancy-postgres-1",
                        "Exited (255) 2 minutes ago",
                    )])
                } else {
                    Err(ContainerExecError::Failure {
                        command: "runtime ps --all".to_owned(),
                        code: None,
                        stdout: String::new(),
                        stderr: "[effigy] command timed out after 30s".to_owned(),
                    })
                }
            },
            |_| {
                Ok(RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
        )
        .expect_err("inspect timeout after start must keep last observed status");

        let detail = error.to_string();
        assert!(detail.contains("service `postgres`"), "got: {detail}");
        assert!(detail.contains("Exited (255)"), "got: {detail}");
        assert!(
            detail.contains("command timed out after 30s"),
            "got: {detail}"
        );
        assert!(
            detail.contains("colima nerdctl --profile effigy -- start acowtancy-postgres-1"),
            "got: {detail}"
        );
    }
}
