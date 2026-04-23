#[path = "system_command/workspace.rs"]
mod workspace;

use effigy_cli::{ContainerArgs, ContainerSubcommand, SystemArgs, SystemSubcommand, WorkspaceArgs};
use effigy_containers::{
    exec::{
        colima_is_running, list_running_compose_containers_for_profile, recover_colima_runtime,
        reset_colima_runtime, ColimaRecoveryReport, ContainerExecError, RunningComposeContainer,
    },
    load_container_policy, validate_container_policy, EffectiveContainerPolicy,
};
use effigy_ui::theme::is_ci_environment;
use effigy_ui::{OutputMode, PlainRenderer, Renderer, SpinnerHandle};
use std::io::IsTerminal;
use std::path::Path;

use crate::runner::command_context::{current_working_dir, resolve_repo_root};
use crate::runner::container_command::run_container;

use super::error::RunnerError;

pub(super) fn run_system(args: SystemArgs) -> Result<String, RunnerError> {
    let cwd = current_working_dir()?;
    let resolved = resolve_repo_root(cwd, args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let container_name = workspace::resolve_public_workspace_container(
        &repo_root,
        args.system.as_deref(),
        None,
        "system",
    )?;
    match args.subcommand {
        SystemSubcommand::Repair => run_system_recovery(
            &repo_root,
            container_name.as_deref(),
            args.output_json,
            true,
        ),
        SystemSubcommand::ResetRuntime => run_system_recovery(
            &repo_root,
            container_name.as_deref(),
            args.output_json,
            false,
        ),
        subcommand => run_system_container_subcommand(
            &repo_root,
            container_name,
            args.repo_override,
            args.output_json,
            subcommand,
        ),
    }
}

pub(in crate::runner) fn run_workspace(args: WorkspaceArgs) -> Result<String, RunnerError> {
    workspace::run_workspace(args)
}

pub(in crate::runner) fn run_workspace_seeded_session(
    repo_root: &Path,
    container_name: Option<&str>,
    repo_override: Option<std::path::PathBuf>,
    initial_command: &str,
) -> Result<String, RunnerError> {
    workspace::run_workspace_seeded_session(
        repo_root,
        container_name,
        repo_override,
        initial_command,
    )
}

fn load_resolved_container_policy(
    repo_root: &std::path::Path,
    container_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, RunnerError> {
    let policy = load_container_policy(repo_root, container_name)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    validate_container_policy(repo_root, &policy)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok(policy)
}

pub(super) fn is_primary_service_running(
    repo_root: &std::path::Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    if !colima_is_running(policy, repo_root).map_err(Into::<RunnerError>::into)? {
        return Ok(false);
    }

    let running = match list_running_compose_containers_for_profile(&policy.profile) {
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

pub(super) fn has_running_primary_service(
    rows: &[RunningComposeContainer],
    repo_root: &std::path::Path,
    project_name: &str,
    primary_service: &str,
) -> bool {
    let repo_root = repo_root.to_string_lossy();
    rows.iter().any(|row| {
        row.project_name.as_deref() == Some(project_name)
            && row.working_dir.as_deref() == Some(repo_root.as_ref())
            && row.service.as_deref() == Some(primary_service)
    })
}

fn exec_error_means_runtime_not_running(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => {
            stderr.contains("level=fatal msg=\"colima is not running\"")
                || stderr.contains("Cannot connect to the Docker daemon")
                || stderr.contains("daemon is not running")
        }
        ContainerExecError::Launch { .. } => false,
    }
}

fn run_system_container_subcommand(
    repo_root: &Path,
    container_name: Option<String>,
    repo_override: Option<std::path::PathBuf>,
    output_json: bool,
    subcommand: SystemSubcommand,
) -> Result<String, RunnerError> {
    let progress_label = system_progress_label(&subcommand);
    let mapped = map_system_container_subcommand(container_name, subcommand);
    let mut progress = SystemProgressReporter::new(output_json, progress_label);
    let result = run_container(ContainerArgs {
        subcommand: mapped,
        repo_override,
        output_json,
    });
    progress.finish(result.is_ok());
    result
}

fn map_system_container_subcommand(
    container_name: Option<String>,
    subcommand: SystemSubcommand,
) -> ContainerSubcommand {
    match subcommand {
        SystemSubcommand::Up => ContainerSubcommand::Up {
            name: container_name,
            attach: false,
            detach: true,
        },
        SystemSubcommand::Down => ContainerSubcommand::Down {
            name: container_name,
        },
        SystemSubcommand::Status => ContainerSubcommand::Status {
            name: container_name,
            all: false,
        },
        SystemSubcommand::Logs { follow } => ContainerSubcommand::Logs {
            name: container_name,
            service: None,
            follow,
        },
        SystemSubcommand::Repair | SystemSubcommand::ResetRuntime => {
            unreachable!("recovery subcommands are handled separately")
        }
    }
}

fn run_system_recovery(
    repo_root: &Path,
    container_name: Option<&str>,
    output_json: bool,
    repair: bool,
) -> Result<String, RunnerError> {
    let policy = load_resolved_container_policy(repo_root, container_name)?;
    let progress_label = if repair {
        system_progress_label(&SystemSubcommand::Repair)
    } else {
        system_progress_label(&SystemSubcommand::ResetRuntime)
    };
    let mut progress = SystemProgressReporter::new(output_json, progress_label);
    let result = if repair {
        render_system_recovery(
            recover_colima_runtime(&policy, repo_root).map_err(Into::<RunnerError>::into),
            output_json,
        )
    } else {
        render_system_recovery(
            reset_colima_runtime(&policy, repo_root).map_err(Into::<RunnerError>::into),
            output_json,
        )
    };
    progress.finish(result.is_ok());
    result
}

fn system_progress_label(subcommand: &SystemSubcommand) -> Option<&'static str> {
    match subcommand {
        SystemSubcommand::Up => Some("System: starting containers"),
        SystemSubcommand::Down => Some("System: stopping containers"),
        SystemSubcommand::Repair => Some("System: recovering Colima runtime"),
        SystemSubcommand::ResetRuntime => Some("System: hard-resetting Colima runtime"),
        SystemSubcommand::Logs { follow: false } => Some("System: collecting logs"),
        SystemSubcommand::Status | SystemSubcommand::Logs { follow: true } => None,
    }
}

fn render_system_recovery(
    result: Result<ColimaRecoveryReport, RunnerError>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let report = result?;
    if output_json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "schema": "effigy.system.recover.v1",
            "schema_version": 1,
            "ok": true,
            "profile": report.profile,
            "steps": report.steps,
        }))
        .map_err(|error| RunnerError::task_invocation(error.to_string()));
    }
    Ok(report.steps.join("\n"))
}

struct SystemProgressReporter {
    spinner: Option<Box<dyn SpinnerHandle>>,
}

impl SystemProgressReporter {
    fn new(output_json: bool, label: Option<&str>) -> Self {
        if output_json || !std::io::stderr().is_terminal() || is_ci_environment() {
            return Self { spinner: None };
        }

        let Some(label) = label else {
            return Self { spinner: None };
        };

        let mut renderer = PlainRenderer::stderr(OutputMode::from_env());
        let spinner = renderer.spinner(label).ok();
        Self { spinner }
    }

    fn finish(&mut self, success: bool) {
        let Some(spinner) = self.spinner.take() else {
            return;
        };
        if success {
            spinner.finish_clear();
        } else {
            spinner.finish_error("System command failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_progress_labels_match_blocking_subcommands() {
        assert_eq!(
            system_progress_label(&SystemSubcommand::Up),
            Some("System: starting containers")
        );
        assert_eq!(
            system_progress_label(&SystemSubcommand::Down),
            Some("System: stopping containers")
        );
        assert_eq!(
            system_progress_label(&SystemSubcommand::Repair),
            Some("System: recovering Colima runtime")
        );
        assert_eq!(
            system_progress_label(&SystemSubcommand::ResetRuntime),
            Some("System: hard-resetting Colima runtime")
        );
        assert_eq!(
            system_progress_label(&SystemSubcommand::Logs { follow: false }),
            Some("System: collecting logs")
        );
        assert_eq!(system_progress_label(&SystemSubcommand::Status), None);
        assert_eq!(
            system_progress_label(&SystemSubcommand::Logs { follow: true }),
            None
        );
    }

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
            },
            RunningComposeContainer {
                container_name: "other-workspace-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: Vec::new(),
                project_name: Some("other".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("workspace".to_owned()),
            },
            RunningComposeContainer {
                container_name: "demo-workspace-1".to_owned(),
                status: "Up 10 seconds".to_owned(),
                ports: Vec::new(),
                project_name: Some("demo".to_owned()),
                working_dir: Some("/tmp/demo".to_owned()),
                service: Some("workspace".to_owned()),
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
        let colima_stopped = ContainerExecError::Failure {
            command: "colima nerdctl ps".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "time=\"2026-04-20T08:31:53+01:00\" level=fatal msg=\"colima is not running\""
                .to_owned(),
        };
        let docker_stopped = ContainerExecError::Failure {
            command: "docker ps".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
                .to_owned(),
        };

        assert!(exec_error_means_runtime_not_running(&colima_stopped));
        assert!(exec_error_means_runtime_not_running(&docker_stopped));
    }
}
