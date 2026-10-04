//! Recover owned Compose services that stay Exited after a Colima VM restart.
//!
//! `nerdctl compose up` can return success while owned containers remain
//! stopped. Manual `nerdctl start` brings them back and may log a stale
//! health-check timer unit. Effigy starts only currently declared services
//! whose Compose project label matches an owned project, skips one-off
//! `compose run` containers and undeclared orphans, does not recreate them
//! (volumes stay), and does not delete systemd units.
//!
//! When a stopped owned Colima/nerdctl container stays down after start and
//! the backend reports a stale health-check timer, Effigy may stop
//! `{full_id}.timer` and `reset-failed` `{full_id}.service`/`{full_id}.timer`
//! after inspect proves the full hexadecimal ID, selected labels, and
//! transient unit identity. Docker start behavior is unchanged.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::colima_runtime::run_runtime_command_capture_for_policy_allow_failure_with_timeout;
use super::healthcheck_timer::{
    expected_ownership_from_row, recover_owned_stale_healthcheck_units, show_unit_invocation,
    CommandOutcome, StaleTimerAction, UnitProbe,
};
use super::implementation::{
    list_compose_containers_for_project_including_stopped, ContainerExecError,
};
use super::parse::{
    compose_status_is_running, compose_status_needs_start, looks_like_stale_healthcheck_timer,
    RunningComposeContainer,
};
use super::process::run_command_capture_allow_failure_with_timeout;
use crate::compose::{resolve_compose_backend_for_repo, ComposeBackend};
use crate::EffectiveContainerPolicy;

const OWNED_SERVICE_START_TIMEOUT: Duration = Duration::from_secs(30);
const OWNED_INSPECT_TIMEOUT: Duration = Duration::from_secs(15);
const HEALTHCHECK_UNIT_TIMEOUT: Duration = Duration::from_secs(10);

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
    let declared = declared_service_names(&compose_files_for_project(policy, project_name))?;
    let declared: Vec<&str> = declared.iter().map(String::as_str).collect();
    recover_exited_owned_services_with_stale_timer(
        project_name,
        &policy.profile,
        backend,
        &declared,
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
        |row, _start_result| {
            if backend != ComposeBackend::ColimaNerdctl {
                return Ok(StaleTimerAction::Skip);
            }
            live_recover_stale_healthcheck_units(repo_root, policy, project_name, &declared, row)
        },
    )
}

#[cfg(test)]
pub fn recover_exited_owned_services_with(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    declared_services: &[&str],
    inspect: impl FnMut() -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
    start: impl FnMut(&str) -> Result<RuntimeStartResult, ContainerExecError>,
) -> Result<OwnedServiceStartRecovery, ContainerExecError> {
    recover_exited_owned_services_with_stale_timer(
        project_name,
        profile,
        backend,
        declared_services,
        inspect,
        start,
        |_, _| Ok(StaleTimerAction::Skip),
    )
}

pub(crate) fn recover_exited_owned_services_with_stale_timer(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    declared_services: &[&str],
    mut inspect: impl FnMut() -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
    mut start: impl FnMut(&str) -> Result<RuntimeStartResult, ContainerExecError>,
    mut recover_timer: impl FnMut(
        &RunningComposeContainer,
        &RuntimeStartResult,
    ) -> Result<StaleTimerAction, ContainerExecError>,
) -> Result<OwnedServiceStartRecovery, ContainerExecError> {
    let before = inspect()?;
    let targets = owned_services_needing_start(project_name, declared_services, &before);
    if targets.is_empty() {
        return Ok(OwnedServiceStartRecovery::default());
    }

    let mut recovery = OwnedServiceStartRecovery::default();
    let mut start_results = Vec::new();
    for row in &targets {
        match start(&row.container_name) {
            Ok(result) => {
                recovery.started.push(row.container_name.clone());
                start_results.push((row.clone(), result));
            }
            Err(error) => {
                start_results.push((
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
                &start_results,
                error,
            ));
        }
    };
    let mut retried = false;
    for (row, result) in &mut start_results {
        let current = after.iter().find(|current| {
            current.container_name == row.container_name
                && current.project_name.as_deref() == Some(project_name)
        });
        let still_down = current
            .map(|row| !compose_status_is_running(&row.status))
            .unwrap_or(true);
        if backend != ComposeBackend::ColimaNerdctl
            || !still_down
            || !looks_like_stale_healthcheck_timer(&result.stdout, &result.stderr)
        {
            continue;
        }
        match recover_timer(row, result)? {
            StaleTimerAction::Skip => {}
            StaleTimerAction::Recovered { warning, .. } => {
                recovery.warnings.push(warning.clone());
                retried = true;
                match start(&row.container_name) {
                    Ok(mut retry) => {
                        if !retry.stderr.is_empty() {
                            retry.stderr.push('\n');
                        }
                        retry.stderr.push_str(&warning);
                        *result = retry;
                    }
                    Err(error) => {
                        *result = RuntimeStartResult {
                            success: false,
                            stdout: String::new(),
                            stderr: format!("{error}\n{warning}"),
                        };
                    }
                }
            }
            StaleTimerAction::Refused { diagnostic } => {
                if !result.stderr.is_empty() {
                    result.stderr.push('\n');
                }
                result.stderr.push_str(&diagnostic);
            }
        }
    }
    let after = if retried {
        match inspect() {
            Ok(rows) => rows,
            Err(error) => {
                return Err(failure_from_last_observed_state(
                    project_name,
                    profile,
                    backend,
                    &targets,
                    &before,
                    &start_results,
                    error,
                ));
            }
        }
    } else {
        after
    };
    if let Some(error) = persistent_start_failure(
        project_name,
        profile,
        backend,
        &targets,
        &after,
        &start_results,
    ) {
        return Err(error);
    }
    for (row, result) in &start_results {
        if looks_like_stale_healthcheck_timer(&result.stdout, &result.stderr)
            && after.iter().any(|current| {
                current.container_name == row.container_name
                    && current.project_name.as_deref() == Some(project_name)
                    && compose_status_is_running(&current.status)
            })
        {
            recovery.warnings.push(stale_timer_warning(
                row,
                &result.stderr,
                result.success,
                profile,
            ));
        }
    }
    Ok(recovery)
}

fn live_recover_stale_healthcheck_units(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    declared_services: &[&str],
    row: &RunningComposeContainer,
) -> Result<StaleTimerAction, ContainerExecError> {
    let Some(expected) = expected_ownership_from_row(
        row,
        project_name,
        policy.profile.as_str(),
        declared_services,
    ) else {
        return Ok(StaleTimerAction::Refused {
            diagnostic: format!(
                "refused stale nerdctl health-check unit recovery for container `{}`\ninspected: id=unknown status={} project={} service=unknown oneoff={} profile={}\nreason: compose row is missing a service label; undeclared orphans are not recovered\nEffigy did not stop, reset, or delete systemd units.\nnext:\n  colima nerdctl --profile {} -- inspect {}\n  colima nerdctl --profile {} -- start {}",
                row.container_name,
                row.status,
                row.project_name.as_deref().unwrap_or("unknown"),
                row.oneoff,
                policy.profile,
                policy.profile,
                row.container_name,
                policy.profile,
                row.container_name
            ),
        });
    };

    let inspect_output = run_runtime_command_capture_for_policy_allow_failure_with_timeout(
        repo_root,
        policy,
        &[
            OsString::from("inspect"),
            OsString::from(row.container_name.as_str()),
        ],
        &format!("inspect owned container `{}`", row.container_name),
        OWNED_INSPECT_TIMEOUT,
    )?;
    let inspect = UnitProbe {
        success: inspect_output.status.success(),
        stdout: String::from_utf8_lossy(&inspect_output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&inspect_output.stderr).into_owned(),
    };

    recover_owned_stale_healthcheck_units(
        expected,
        &inspect,
        |unit| {
            let (program, args) = show_unit_invocation(policy.profile.as_str(), unit);
            run_vm_command_allow_failure(
                repo_root,
                &program,
                &args,
                &format!("systemctl show {unit}"),
            )
            .map(|outcome| UnitProbe {
                success: outcome.success,
                stdout: outcome.stdout,
                stderr: outcome.stderr,
            })
        },
        |program, args| {
            run_vm_command_allow_failure(
                repo_root,
                &OsString::from(program),
                args,
                "systemctl health-check unit recovery",
            )
        },
        std::thread::sleep,
    )
}

fn run_vm_command_allow_failure(
    repo_root: &Path,
    program: &OsString,
    args: &[OsString],
    label: &str,
) -> Result<CommandOutcome, ContainerExecError> {
    let program_name = program.to_string_lossy().into_owned();
    let rendered: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let borrowed: Vec<&str> = rendered.iter().map(String::as_str).collect();
    let output = run_command_capture_allow_failure_with_timeout(
        repo_root,
        &program_name,
        &borrowed,
        label,
        HEALTHCHECK_UNIT_TIMEOUT,
    )?;
    Ok(CommandOutcome {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn owned_services_needing_start(
    project_name: &str,
    declared_services: &[&str],
    rows: &[RunningComposeContainer],
) -> Vec<RunningComposeContainer> {
    let declared: BTreeSet<&str> = declared_services.iter().copied().collect();
    rows.iter()
        .filter(|row| row.project_name.as_deref() == Some(project_name))
        .filter(|row| {
            row.service
                .as_deref()
                .is_some_and(|service| declared.contains(service))
        })
        .filter(|row| !is_compose_oneoff(row))
        .filter(|row| compose_status_needs_start(&row.status))
        .cloned()
        .collect()
}

fn compose_files_for_project(
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Vec<PathBuf> {
    if project_name == policy.project_name {
        return policy.compose_files.clone();
    }
    policy
        .shared_services
        .iter()
        .filter(|shared| shared.project_name == project_name)
        .map(|shared| shared.compose_file.clone())
        .collect()
}

fn declared_service_names(
    compose_files: &[PathBuf],
) -> Result<BTreeSet<String>, ContainerExecError> {
    let mut names = BTreeSet::new();
    for compose_file in compose_files {
        let content =
            std::fs::read_to_string(compose_file).map_err(|error| ContainerExecError::Failure {
                command: format!("read compose file {}", compose_file.display()),
                code: None,
                stdout: String::new(),
                stderr: error.to_string(),
            })?;
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|error| ContainerExecError::Failure {
                command: format!("parse compose file {}", compose_file.display()),
                code: None,
                stdout: String::new(),
                stderr: error.to_string(),
            })?;
        let Some(services) = parsed
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for key in services.keys() {
            if let Some(name) = key.as_str() {
                names.insert(name.to_owned());
            }
        }
    }
    Ok(names)
}

fn is_compose_oneoff(row: &RunningComposeContainer) -> bool {
    if row.oneoff {
        return true;
    }
    let Some(service) = row.service.as_deref() else {
        return false;
    };
    let name = row.container_name.as_str();
    name.contains(&format!("-{service}-run-")) || name.contains(&format!("_{service}_run_"))
}

fn persistent_start_failure(
    project_name: &str,
    profile: &str,
    backend: ComposeBackend,
    targets: &[RunningComposeContainer],
    after: &[RunningComposeContainer],
    start_results: &[(RunningComposeContainer, RuntimeStartResult)],
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
        let backend_text = start_results
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
    start_results: &[(RunningComposeContainer, RuntimeStartResult)],
    inspect_error: ContainerExecError,
) -> ContainerExecError {
    let inspect_text = inspect_error.to_string();
    let mut start_results = start_results.to_vec();
    for target in targets {
        match start_results
            .iter_mut()
            .find(|(row, _)| row.container_name == target.container_name)
        {
            Some((_, result)) => {
                if !result.stderr.is_empty() {
                    result.stderr.push('\n');
                }
                result.stderr.push_str(&inspect_text);
            }
            None => start_results.push((
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
        &start_results,
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
    profile: &str,
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
            "unresolved nerdctl health-check unit collision while starting owned service `{service}` (`{}`): inspect shows the container running, but the failed start did not prove health-check readiness. Inspect with `{}` and retry the exact-owned pair with `{}` once the timer is unloaded; systemd units were not deleted",
            excerpt_timer_line(stderr),
            owned_inspect_recovery_command(ComposeBackend::ColimaNerdctl, profile, &row.container_name),
            owned_start_recovery_command(ComposeBackend::ColimaNerdctl, profile, &row.container_name),
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

fn owned_inspect_recovery_command(
    backend: ComposeBackend,
    profile: &str,
    container_name: &str,
) -> String {
    match backend {
        ComposeBackend::Docker => format!("docker inspect {container_name}"),
        ComposeBackend::ColimaNerdctl => {
            format!("colima nerdctl --profile {profile} -- inspect {container_name}")
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
            oneoff: false,
        }
    }

    fn oneoff_row(
        project: &str,
        service: &str,
        container: &str,
        status: &str,
    ) -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: container.to_owned(),
            status: status.to_owned(),
            ports: Vec::new(),
            project_name: Some(project.to_owned()),
            working_dir: Some("/tmp/acowtancy".to_owned()),
            service: Some(service.to_owned()),
            oneoff: true,
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
            &["postgres"],
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
            &["mysql"],
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
    fn successful_start_with_stale_timer_still_exited_is_a_bounded_failure() {
        let inspect_states = [
            vec![row(
                "acowtancy-shared-mysql",
                "mysql",
                "acowtancy-mysql-1",
                "Created",
            )],
            vec![row(
                "acowtancy-shared-mysql",
                "mysql",
                "acowtancy-mysql-1",
                "Exited (255) 1 second ago",
            )],
        ];
        let mut inspect_calls = 0;
        let error = recover_exited_owned_services_with(
            "acowtancy-shared-mysql",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["mysql"],
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |_| {
                Ok(RuntimeStartResult {
                    success: true,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
        )
        .expect_err("start exit 0 is not readiness when the service stays down");

        let detail = error.to_string();
        assert!(detail.contains("service `mysql`"), "got: {detail}");
        assert!(detail.contains("Exited (255)"), "got: {detail}");
        assert!(
            detail.contains("colima nerdctl --profile effigy -- start acowtancy-mysql-1"),
            "got: {detail}"
        );
        assert!(
            detail.contains("stale nerdctl health-check timer"),
            "got: {detail}"
        );
        assert!(
            detail.contains("Unit dd94022f7dd0.timer was already loaded or has a fragment file"),
            "got: {detail}"
        );
        assert!(
            detail.contains("will not delete timer units"),
            "got: {detail}"
        );
        assert!(
            !detail.contains("the container is running"),
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
            &["app"],
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
    fn declared_stopped_service_recovers_while_oneoff_and_orphan_are_left_alone() {
        let inspect_states = [
            vec![
                row(
                    "acowtancy-shared-pg",
                    "postgres",
                    "acowtancy-postgres-1",
                    "Exited (255) 2 minutes ago",
                ),
                oneoff_row(
                    "acowtancy-shared-pg",
                    "postgres",
                    "acowtancy-postgres-run-deadbeef",
                    "Exited (0) 1 second ago",
                ),
                row(
                    "acowtancy-shared-pg",
                    "legacy-redis",
                    "acowtancy-legacy-redis-1",
                    "Exited (255) 2 minutes ago",
                ),
            ],
            vec![
                row(
                    "acowtancy-shared-pg",
                    "postgres",
                    "acowtancy-postgres-1",
                    "Up 1 second",
                ),
                oneoff_row(
                    "acowtancy-shared-pg",
                    "postgres",
                    "acowtancy-postgres-run-deadbeef",
                    "Exited (0) 1 second ago",
                ),
                row(
                    "acowtancy-shared-pg",
                    "legacy-redis",
                    "acowtancy-legacy-redis-1",
                    "Exited (255) 2 minutes ago",
                ),
            ],
        ];
        let mut inspect_calls = 0;
        let mut started = Vec::new();
        let recovery = recover_exited_owned_services_with(
            "acowtancy-shared-pg",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["postgres"],
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
        .expect("declared stopped service should recover");

        assert_eq!(started, vec!["acowtancy-postgres-1".to_owned()]);
        assert_eq!(recovery.started, vec!["acowtancy-postgres-1".to_owned()]);
        assert!(!started.iter().any(|name| name.contains("-run-")));
        assert!(!started.iter().any(|name| name.contains("legacy-redis")));
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
            &["postgres"],
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
            &["app"],
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
            &["app"],
            || {
                Ok(vec![RunningComposeContainer {
                    container_name: "mystery-1".to_owned(),
                    status: "Exited (0) 1 second ago".to_owned(),
                    ports: Vec::new(),
                    project_name: None,
                    working_dir: None,
                    service: None,
                    oneoff: false,
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
            &["postgres"],
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
            &["postgres"],
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
        assert!(
            detail.contains("stale nerdctl health-check timer"),
            "got: {detail}"
        );
    }

    #[test]
    fn inspect_timeout_after_successful_start_keeps_stale_timer_diagnosis() {
        let mut inspect_calls = 0;
        let error = recover_exited_owned_services_with(
            "acowtancy-shared-pg",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["postgres"],
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
                    success: true,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
        )
        .expect_err("inspect timeout after start exit 0 must keep the start diagnosis");

        let detail = error.to_string();
        assert!(detail.contains("service `postgres`"), "got: {detail}");
        assert!(detail.contains("Exited (255)"), "got: {detail}");
        assert!(
            detail.contains("command timed out after 30s"),
            "got: {detail}"
        );
        assert!(
            detail.contains("stale nerdctl health-check timer"),
            "got: {detail}"
        );
        assert!(
            detail.contains("Unit dd94022f7dd0.timer was already loaded or has a fragment file"),
            "got: {detail}"
        );
        assert!(
            detail.contains("colima nerdctl --profile effigy -- start acowtancy-postgres-1"),
            "got: {detail}"
        );
        assert!(
            !detail.contains("the container is running"),
            "got: {detail}"
        );
    }

    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn recovered_action() -> StaleTimerAction {
        StaleTimerAction::Recovered {
            warning: format!(
                "recovered stale nerdctl health-check timer for owned service `mysql` (`{FULL_ID}`); stopped `{FULL_ID}.timer` and reset-failed `{FULL_ID}.service`/`{FULL_ID}.timer` without deleting units"
            ),
            volume_refs: Vec::new(),
            stop_timer: vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from("effigy"),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("systemctl"),
                OsString::from("stop"),
                OsString::from(format!("{FULL_ID}.timer")),
            ],
            reset_failed: vec![
                OsString::from("ssh"),
                OsString::from("--profile"),
                OsString::from("effigy"),
                OsString::from("--"),
                OsString::from("sudo"),
                OsString::from("systemctl"),
                OsString::from("reset-failed"),
                OsString::from(format!("{FULL_ID}.service")),
                OsString::from(format!("{FULL_ID}.timer")),
            ],
        }
    }

    #[test]
    fn stale_timer_collision_recovers_exact_units_then_retries_until_inspect_ready() {
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
            vec![row(
                "acowtancy-shared-mysql",
                "mysql",
                "acowtancy-mysql-1",
                "Up 1 second",
            )],
        ];
        let mut inspect_calls = 0;
        let mut started = Vec::new();
        let mut recoveries = 0;
        let recovery = recover_exited_owned_services_with_stale_timer(
            "acowtancy-shared-mysql",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["mysql"],
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |container| {
                started.push(container.to_owned());
                if started.len() == 1 {
                    Ok(RuntimeStartResult {
                        success: false,
                        stdout: String::new(),
                        stderr: STALE_TIMER.to_owned(),
                    })
                } else {
                    Ok(RuntimeStartResult {
                        success: true,
                        stdout: container.to_owned(),
                        stderr: String::new(),
                    })
                }
            },
            |row, result| {
                recoveries += 1;
                assert_eq!(row.container_name, "acowtancy-mysql-1");
                assert!(looks_like_stale_healthcheck_timer(
                    &result.stdout,
                    &result.stderr
                ));
                Ok(recovered_action())
            },
        )
        .expect("exact-owned stale timer should recover and retry");

        assert_eq!(
            started,
            vec![
                "acowtancy-mysql-1".to_owned(),
                "acowtancy-mysql-1".to_owned()
            ]
        );
        assert_eq!(recoveries, 1);
        assert_eq!(inspect_calls, 3);
        assert_eq!(recovery.started, vec!["acowtancy-mysql-1".to_owned()]);
        assert_eq!(recovery.warnings.len(), 1);
        assert!(recovery.warnings[0].contains(FULL_ID));
        assert!(recovery.warnings[0].contains("without deleting units"));
        if let StaleTimerAction::Recovered {
            stop_timer,
            reset_failed,
            ..
        } = recovered_action()
        {
            let timer = format!("{FULL_ID}.timer");
            let service = format!("{FULL_ID}.service");
            let stop = stop_timer
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let reset = reset_failed
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert_eq!(stop.last(), Some(&timer));
            assert!(!stop.contains(&service));
            assert!(reset.contains(&service));
            assert!(reset.contains(&timer));
            assert!(!stop.iter().any(|arg| arg.contains('*') || arg == "rm"));
        }
    }

    #[test]
    fn refused_persistent_unit_does_not_retry_start_and_keeps_diagnostics() {
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
        let mut started = Vec::new();
        let error = recover_exited_owned_services_with_stale_timer(
            "acowtancy-shared-mysql",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["mysql"],
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |container| {
                started.push(container.to_owned());
                Ok(RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
            |_, _| {
                Ok(StaleTimerAction::Refused {
                    diagnostic: format!(
                        "refused stale nerdctl health-check unit recovery for container `acowtancy-mysql-1`\ninspected: id={FULL_ID} status=exited project=acowtancy-shared-mysql service=mysql oneoff=false profile=effigy\nreason: unit `{FULL_ID}.timer` has a persistent fragment at `/etc/systemd/system/{FULL_ID}.timer`\nEffigy did not stop, reset, or delete systemd units.\nnext:\n  colima nerdctl --profile effigy -- inspect acowtancy-mysql-1\n  colima nerdctl --profile effigy -- start acowtancy-mysql-1"
                    ),
                })
            },
        )
        .expect_err("persistent unit must not be recovered");

        assert_eq!(started, vec!["acowtancy-mysql-1".to_owned()]);
        assert_eq!(inspect_calls, 2);
        let detail = error.to_string();
        assert!(detail.contains("service `mysql`"), "got: {detail}");
        assert!(detail.contains("persistent fragment"), "got: {detail}");
        assert!(detail.contains(FULL_ID), "got: {detail}");
        assert!(
            detail.contains("did not stop, reset, or delete"),
            "got: {detail}"
        );
        assert!(
            detail.contains("colima nerdctl --profile effigy -- start acowtancy-mysql-1"),
            "got: {detail}"
        );
        assert!(!detail.contains("rm -f"), "got: {detail}");
        assert!(!detail.contains("daemon-reload"), "got: {detail}");
    }

    #[test]
    fn recovered_collision_that_stays_down_after_one_retry_is_bounded() {
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
            vec![row(
                "acowtancy-shared-mysql",
                "mysql",
                "acowtancy-mysql-1",
                "Exited (255) 1 second ago",
            )],
        ];
        let mut inspect_calls = 0;
        let mut started = Vec::new();
        let mut recoveries = 0;
        let error = recover_exited_owned_services_with_stale_timer(
            "acowtancy-shared-mysql",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["mysql"],
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |container| {
                started.push(container.to_owned());
                Ok(RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
            |_, _| {
                recoveries += 1;
                Ok(recovered_action())
            },
        )
        .expect_err("one retry is bounded");

        assert_eq!(started.len(), 2);
        assert_eq!(recoveries, 1);
        assert_eq!(inspect_calls, 3);
        let detail = error.to_string();
        assert!(detail.contains("service `mysql`"), "got: {detail}");
        assert!(detail.contains("Exited (255)"), "got: {detail}");
        assert!(detail.contains("without deleting units"), "got: {detail}");
        assert!(
            detail.contains("will not delete timer units"),
            "got: {detail}"
        );
    }

    #[test]
    fn docker_backend_does_not_recover_healthcheck_units() {
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
        let error = recover_exited_owned_services_with_stale_timer(
            "demo-web",
            "effigy",
            ComposeBackend::Docker,
            &["app"],
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
            |_, _| panic!("docker must not recover nerdctl health-check units"),
        )
        .expect_err("docker persistent failure");

        let detail = error.to_string();
        assert!(detail.contains("docker start demo-app-1"), "got: {detail}");
        assert!(!detail.contains("reset-failed"), "got: {detail}");
        assert!(!detail.contains("systemctl stop"), "got: {detail}");
    }

    #[test]
    fn pathological_start_fatal_with_running_container_reports_unresolved_healthcheck() {
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
                "Up 1 second",
            )],
        ];
        let mut inspect_calls = 0;
        let mut started = Vec::new();
        let mut recoveries = 0;
        let recovery = recover_exited_owned_services_with_stale_timer(
            "acowtancy-shared-mysql",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["mysql"],
            || {
                let rows = inspect_states[inspect_calls.min(inspect_states.len() - 1)].clone();
                inspect_calls += 1;
                Ok(rows)
            },
            |container| {
                started.push(container.to_owned());
                Ok(RuntimeStartResult {
                    success: false,
                    stdout: String::new(),
                    stderr: STALE_TIMER.to_owned(),
                })
            },
            |_, _| {
                recoveries += 1;
                panic!("a now-running container must not be mutated");
            },
        )
        .expect("a running container is not a failed service");

        assert_eq!(started.len(), 1, "the single start is not retried");
        assert_eq!(recoveries, 0, "no unit mutation on a now-running container");
        assert_eq!(inspect_calls, 2);
        assert_eq!(recovery.started, vec!["acowtancy-mysql-1".to_owned()]);
        assert_eq!(recovery.warnings.len(), 1);
        let warning = &recovery.warnings[0];
        assert!(
            warning.contains("unresolved nerdctl health-check unit collision"),
            "got: {warning}"
        );
        assert!(
            warning.contains("did not prove health-check readiness"),
            "got: {warning}"
        );
        assert!(
            warning.contains("Unit dd94022f7dd0.timer was already loaded"),
            "got: {warning}"
        );
        assert!(
            warning.contains("colima nerdctl --profile effigy -- inspect acowtancy-mysql-1"),
            "got: {warning}"
        );
        assert!(
            warning.contains("colima nerdctl --profile effigy -- start acowtancy-mysql-1"),
            "got: {warning}"
        );
        assert!(
            !warning.contains("the container is running and systemd units were not deleted"),
            "running alone must not be reported as clean recovery: {warning}"
        );
    }

    #[test]
    fn running_container_never_invokes_timer_recovery() {
        let mut recoveries = 0;
        let recovery = recover_exited_owned_services_with_stale_timer(
            "demo-web",
            "effigy",
            ComposeBackend::ColimaNerdctl,
            &["app"],
            || Ok(vec![row("demo-web", "app", "demo-app-1", "Up 10 seconds")]),
            |container| panic!("start must not run for running containers, got {container}"),
            |_, _| {
                recoveries += 1;
                panic!("running containers must not recover health-check units");
            },
        )
        .expect("running owned service is left alone");
        assert_eq!(recoveries, 0);
        assert!(recovery.started.is_empty());
    }
}
