//! Colima lifecycle command specifications.
//!
//! Produces command specs for Colima operations (start, status) and
//! Docker Compose operations (up, down, kill, ps). The caller handles
//! actual process execution.

use crate::compose::{compose_args, resolve_compose_backend, ComposeBackend};
use crate::EffectiveContainerPolicy;
use effigy_manifest::ManifestContainerShutdownMode;
use std::ffi::OsString;

/// A command specification to be executed by the runner.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// Program to run.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Human-readable label for error messages.
    pub label: String,
    /// Whether failure is acceptable (e.g., status check).
    pub allow_failure: bool,
}

/// Build the command to check if a Colima profile is running.
pub fn colima_status_command(policy: &EffectiveContainerPolicy) -> CommandSpec {
    CommandSpec {
        program: "colima".to_string(),
        args: vec![
            "status".to_string(),
            "--profile".to_string(),
            policy.profile.clone(),
        ],
        label: "colima status".to_string(),
        allow_failure: true,
    }
}

/// Build the command to start a Colima profile.
pub fn colima_start_command(policy: &EffectiveContainerPolicy) -> CommandSpec {
    let runtime = match resolve_compose_backend() {
        ComposeBackend::Docker => "docker",
        ComposeBackend::ColimaNerdctl => "containerd",
    };
    CommandSpec {
        program: "colima".to_string(),
        args: vec![
            "start".to_string(),
            "--profile".to_string(),
            policy.profile.clone(),
            "--runtime".to_string(),
            runtime.to_string(),
        ],
        label: "colima start".to_string(),
        allow_failure: false,
    }
}

/// Parse Colima status output to determine if the profile is running.
pub fn parse_colima_running(stdout: &str, stderr: &str) -> bool {
    let stdout_lower = stdout.to_ascii_lowercase();
    let stderr_lower = stderr.to_ascii_lowercase();
    stdout_lower.contains("running") || stderr_lower.contains("running")
}

/// Build the compose args for bringing services up in detached mode.
pub fn compose_up_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    compose_args(policy, ["up", "-d"])
}

/// Build the compose args for checking service status.
pub fn compose_ps_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    compose_args(policy, ["ps"])
}

/// Build the compose args for tearing down with volume removal.
pub fn compose_reset_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    compose_args(policy, ["down", "-v", "--remove-orphans"])
}

/// Build the ordered list of compose args for shutting down a container
/// according to its configured shutdown policy.
///
/// Graceful: `docker compose down --remove-orphans`
/// Immediate: `docker compose kill` then `docker compose down --remove-orphans`
pub fn shutdown_compose_commands(
    policy: &EffectiveContainerPolicy,
) -> Vec<(Vec<OsString>, &'static str)> {
    match policy.shutdown {
        ManifestContainerShutdownMode::Graceful => {
            vec![(
                compose_args(policy, ["down", "--remove-orphans"]),
                "docker compose down",
            )]
        }
        ManifestContainerShutdownMode::Immediate => {
            vec![
                (compose_args(policy, ["kill"]), "docker compose kill"),
                (
                    compose_args(policy, ["down", "--remove-orphans"]),
                    "docker compose down",
                ),
            ]
        }
    }
}

/// Build compose args for following logs of a specific service.
pub fn compose_logs_follow_args(policy: &EffectiveContainerPolicy, service: &str) -> Vec<OsString> {
    compose_args(policy, ["logs", "--follow", service])
}

/// Build compose args for following logs with tail.
pub fn compose_logs_follow_tail_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    tail: &str,
) -> Vec<OsString> {
    compose_args(policy, ["logs", "--follow", "--tail", tail, service])
}

/// Build compose args for fetching recent logs.
pub fn compose_logs_tail_args<'a>(
    policy: &EffectiveContainerPolicy,
    service: &'a str,
    tail: &'a str,
) -> Vec<OsString> {
    compose_args(policy, ["logs", "--tail", tail, service])
}

/// Build compose args for exec into a service with a shell.
pub fn compose_exec_shell_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    shell: &str,
) -> Vec<OsString> {
    compose_args(policy, ["exec", service, shell])
}

/// Build compose args for exec into a service with a command.
pub fn compose_exec_command_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
    command: &str,
) -> Vec<OsString> {
    let mut args = compose_args(policy, ["exec", service, "sh", "-lc"]);
    args.push(OsString::from(command));
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_colima_running_detects_running() {
        assert!(parse_colima_running("INFO[0000] colima is running\n", ""));
        assert!(parse_colima_running("", "colima is running"));
        assert!(parse_colima_running("RUNNING", ""));
    }

    #[test]
    fn parse_colima_running_detects_not_running() {
        assert!(!parse_colima_running("", ""));
        assert!(!parse_colima_running("colima is stopped", ""));
        assert!(!parse_colima_running(
            "error: not found",
            "profile does not exist"
        ));
    }

    #[test]
    fn colima_status_command_uses_profile() {
        let policy = test_policy("myprofile");
        let cmd = colima_status_command(&policy);
        assert_eq!(cmd.program, "colima");
        assert!(cmd.args.contains(&"myprofile".to_string()));
        assert!(cmd.allow_failure);
    }

    #[test]
    fn colima_start_command_uses_profile() {
        let policy = test_policy("myprofile");
        let cmd = colima_start_command(&policy);
        assert_eq!(cmd.program, "colima");
        assert!(cmd.args.contains(&"myprofile".to_string()));
        assert!(!cmd.allow_failure);
    }

    #[test]
    fn shutdown_graceful_produces_one_command() {
        let mut policy = test_policy("default");
        policy.shutdown = ManifestContainerShutdownMode::Graceful;
        let cmds = shutdown_compose_commands(&policy);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].1, "docker compose down");
    }

    #[test]
    fn shutdown_immediate_produces_two_commands() {
        let mut policy = test_policy("default");
        policy.shutdown = ManifestContainerShutdownMode::Immediate;
        let cmds = shutdown_compose_commands(&policy);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].1, "docker compose kill");
        assert_eq!(cmds[1].1, "docker compose down");
    }

    fn test_policy(profile: &str) -> EffectiveContainerPolicy {
        use effigy_manifest::{
            ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerStartup,
        };
        EffectiveContainerPolicy {
            name: "test".to_string(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Attached,
            profile: profile.to_string(),
            compose_file: std::path::PathBuf::from("docker-compose.yml"),
            compose_file_display: "docker-compose.yml".to_string(),
            project_name: "test-project".to_string(),
            primary_service: "app".to_string(),
            declared_ports: vec![],
            declared_mounts: vec![],
            health_check: None,
            health_timeout_secs: 60,
            ui_tabs: vec![],
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
        }
    }
}
