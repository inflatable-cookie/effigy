//! Read-only liveness probes for a container environment's primary service.
//!
//! `container status`, doctor, routed-task preparation and managed pipelines
//! share these probes so "is the stack up?" has one answer.

use effigy_containers::{
    exec::{
        colima_is_running_with_deadline, list_running_compose_containers_for_policy_with_deadline,
        ContainerExecError, RunningComposeContainer,
    },
    EffectiveContainerPolicy,
};
use std::path::Path;
use std::time::Instant;

use super::super::error::RunnerError;

pub(in crate::runner) fn is_primary_service_running(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    is_primary_service_running_with_deadline(repo_root, policy, None)
}

/// Deadline-aware primary-service liveness probe. Every preliminary probe
/// (Colima status, then compose `ps`) converts the same monotonic deadline, so
/// an expired deadline never spawns and a slow first probe consumes the budget
/// the second one would otherwise have had.
pub(in crate::runner) fn is_primary_service_running_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<bool, RunnerError> {
    // Test-only deterministic seam: when the scripted doctor runtime is active
    // it answers the raw Colima `status` and compose `ps` captures and this
    // production function still owns the order, short-circuit and parsing.
    #[cfg(test)]
    if let Some(scripted) = sequence_scripted_liveness(repo_root, policy, deadline) {
        return scripted;
    }
    if !colima_is_running_with_deadline(policy, repo_root, deadline)
        .map_err(Into::<RunnerError>::into)?
    {
        return Ok(false);
    }

    let running =
        match list_running_compose_containers_for_policy_with_deadline(repo_root, policy, deadline)
        {
            Ok(running) => running,
            Err(error) if exec_error_means_runtime_not_running(&error) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
    Ok(has_running_primary_service(
        &running,
        repo_root,
        &policy.project_name,
        &policy.primary_service,
    ))
}

/// Deterministic liveness sequencing for the scripted doctor oracles. Uses the
/// same order (status, then compose `ps` only while running), the same
/// short-circuit, and the production parsers `parse_colima_running` and
/// `parse_running_compose_containers`. Returns `None` when no scripted runtime
/// is installed so production keeps its real path.
#[cfg(test)]
fn sequence_scripted_liveness(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Option<Result<bool, RunnerError>> {
    use effigy_containers::colima::parse_colima_running;
    use effigy_containers::exec::parse_running_compose_containers;

    let status = crate::runner::scripted_doctor::intercept_colima_status(policy, deadline)?;
    let sequenced = (|| -> Result<bool, RunnerError> {
        let status = status?;
        if !status.status.success() {
            return Ok(false);
        }
        let stdout = String::from_utf8_lossy(&status.stdout);
        let stderr = String::from_utf8_lossy(&status.stderr);
        if !parse_colima_running(&stdout, &stderr) {
            return Ok(false);
        }
        let ps = crate::runner::scripted_doctor::intercept_liveness_ps(policy, deadline)
            .expect("scripted runtime answers the compose ps probe after a running status")?;
        let rows = parse_running_compose_containers(&String::from_utf8_lossy(&ps.stdout))
            .map_err(RunnerError::from)?;
        Ok(has_running_primary_service(
            &rows,
            repo_root,
            &policy.project_name,
            &policy.primary_service,
        ))
    })();
    Some(sequenced)
}

pub(super) fn has_running_primary_service(
    rows: &[RunningComposeContainer],
    repo_root: &Path,
    project_name: &str,
    primary_service: &str,
) -> bool {
    rows.iter().any(|row| {
        if row.project_name.as_deref() != Some(project_name)
            || row.service.as_deref() != Some(primary_service)
        {
            return false;
        }
        match row.working_dir.as_deref() {
            Some(working_dir) => {
                effigy_runtime::read::working_dir_belongs_to_repo(working_dir, repo_root)
            }
            // nerdctl-compose (colima backend) does not emit the
            // `com.docker.compose.project.working_dir` label, so the running
            // row carries no working_dir. project_name + service is then the
            // strongest signal we have; trust it.
            None => true,
        }
    })
}

pub(super) fn exec_error_means_runtime_not_running(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => {
            stderr.contains("level=fatal msg=\"colima is not running\"")
                || (stderr.contains("level=fatal msg=\"colima [profile=")
                    && stderr.contains("] is not running\""))
                || stderr.contains("Cannot connect to the Docker daemon")
                || stderr.contains("daemon is not running")
        }
        ContainerExecError::Launch { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_primary_service_requires_repo_project_and_service_match() {
        let repo_root = std::path::Path::new("/tmp/demo");
        let rows = vec![
            RunningComposeContainer {
                container_name: "demo-postgres-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: Vec::new(),
                project_name: Some("demo".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("postgres".to_owned()),
                oneoff: false,
            },
            RunningComposeContainer {
                container_name: "other-workspace-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: Vec::new(),
                project_name: Some("other".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("workspace".to_owned()),
                oneoff: false,
            },
            RunningComposeContainer {
                container_name: "demo-workspace-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: Vec::new(),
                project_name: Some("demo".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("workspace".to_owned()),
                oneoff: false,
            },
        ];

        assert!(has_running_primary_service(
            &rows,
            repo_root,
            "demo",
            "workspace"
        ));
        assert!(!has_running_primary_service(
            &rows, repo_root, "demo", "api"
        ));
        assert!(!has_running_primary_service(
            &rows,
            std::path::Path::new("/tmp/other"),
            "demo",
            "workspace"
        ));
    }

    #[test]
    fn runtime_not_running_errors_degrade_to_false() {
        let failure = |stderr: &str| ContainerExecError::Failure {
            command: "docker ps".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        };
        // Only a runtime that is provably not running degrades; any other
        // failure stays a failure, so a real break is never hidden.
        for stderr in [
            "time=\"2026-04-20T08:31:53+01:00\" level=fatal msg=\"colima is not running\"",
            "time=\"2026-05-13T11:09:48+01:00\" level=fatal msg=\"colima [profile=effigy] is not running\"",
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
            "the daemon is not running",
        ] {
            assert!(exec_error_means_runtime_not_running(&failure(stderr)), "{stderr}");
        }
        for stderr in [
            "level=fatal msg=\"permission denied while trying to connect to the docker API\"",
            "no such container: demo-workspace-1",
        ] {
            assert!(!exec_error_means_runtime_not_running(&failure(stderr)), "{stderr}");
        }
        assert!(!exec_error_means_runtime_not_running(
            &ContainerExecError::Launch {
                command: "colima nerdctl ps".to_owned(),
                error: std::io::Error::other("spawn failed"),
            }
        ));
    }
}
