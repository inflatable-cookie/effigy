//! Container-domain execution helpers extracted from
//! `src/runner/container_command.rs`.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use super::colima_runtime::{
    default_runtime_profile, detect_container_backend,
    run_runtime_command_capture_for_policy_with_absolute_deadline,
    run_runtime_command_capture_for_policy_with_timeout, running_colima_profiles,
};
use super::parse::{
    infer_host_working_dir_from_inspect, parse_running_compose_containers,
    parse_running_container_stats, RunningComposeContainer, RunningComposeContainerProfiled,
    RunningContainerStatsCapture,
};
use super::participation::{
    colima_runtime_participation, docker_runtime_participation, RuntimeParticipation,
};
use super::process::{
    run_command_capture_os, run_command_capture_os_with_env,
    run_command_capture_with_timeout_and_env, run_detached_command_streaming_with_env,
    run_detached_command_with_env,
};

use crate::{
    colima::shutdown_compose_commands, compose::compose_invocation_for_repo, BackendId,
    ContainerBackendDetection, ContainerManager, ContainerManagerError, EffectiveContainerPolicy,
};

const DOCKER_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.service\"}}\t{{.Label \"com.docker.compose.oneoff\"}}";
const DOCKER_STATS_FORMAT: &str = "{{ json . }}";
const CONTAINER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);
const COMPOSE_PS_TIMEOUT: Duration = Duration::from_secs(30);
/// One bounded retry for the stopped-inclusive project inventory. The read is
/// a pure observation, so a retry cannot duplicate an effect, and a second
/// timeout still refuses: this is not a longer timeout.
const INVENTORY_RETRY_BACKOFF: Duration = Duration::from_millis(500);
/// Bound for stopping and removing one stale owned container a deleted or
/// restarted profile left behind.
const DOCKER_GLOBAL_RUNTIME_LABEL: &str = "docker";

#[derive(Debug)]
pub enum ContainerExecError {
    Launch {
        command: String,
        error: std::io::Error,
    },
    /// An observation refused the evidence it read. No command ended, so the
    /// diagnostic must never borrow a process exit code or claim a signal: it
    /// names the observation and the concrete reason it refused.
    Refusal { command: String, reason: String },
    Failure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl ContainerExecError {
    /// A validation refusal with its concrete cause. Only a real child
    /// termination may carry an exit code or a signal claim.
    fn refused(command: &str, reason: impl Into<String>) -> Self {
        Self::Refusal {
            command: command.to_owned(),
            reason: reason.into(),
        }
    }
}

fn container_manager_error(error: ContainerManagerError) -> ContainerExecError {
    ContainerExecError::Failure {
        command: "container manager backend selection".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: error.to_string(),
    }
}

impl std::fmt::Display for ContainerExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch { command, error } => {
                write!(f, "failed to launch `{command}`: {error}")
            }
            Self::Refusal { command, reason } => {
                write!(f, "{command} refused: {reason}")
            }
            Self::Failure {
                command,
                code,
                stdout,
                stderr,
            } => {
                write!(
                    f,
                    "{command} failed ({})\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    effigy_ui::human::exit_code(*code)
                )
            }
        }
    }
}

impl std::error::Error for ContainerExecError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInventoryFailure {
    pub backend: String,
    pub profile: String,
    pub error: String,
    /// The profile's VM is stopped rather than broken. Its created or stopped
    /// containers cannot be listed without starting it, but it publishes no
    /// host port while it is stopped, so only owners recorded on that profile
    /// are uncertain (effigy#294).
    pub stopped_profile: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunningComposeContainerInventory {
    pub rows: Vec<RunningComposeContainerProfiled>,
    pub failures: Vec<RuntimeInventoryFailure>,
}

impl RunningComposeContainerInventory {
    pub fn is_complete(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn failure_summary(&self) -> Option<String> {
        (!self.failures.is_empty()).then(|| {
            self.failures
                .iter()
                .map(|failure| {
                    format!(
                        "{} profile `{}`: {}",
                        failure.backend, failure.profile, failure.error
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    /// Colima profiles whose VM is stopped. Their containers cannot be listed
    /// without starting the profile, so an owner that may live there is
    /// uncertain; every other failure makes the inventory incomplete for
    /// every owner.
    pub fn stopped_profiles(&self) -> Vec<&str> {
        self.failures
            .iter()
            .filter(|failure| failure.stopped_profile)
            .map(|failure| failure.profile.as_str())
            .collect()
    }

    /// Failures that stop the inventory from proving absence for any owner.
    pub fn incomplete_failure_summary(&self) -> Option<String> {
        let failures = self
            .failures
            .iter()
            .filter(|failure| !failure.stopped_profile)
            .collect::<Vec<_>>();
        (!failures.is_empty()).then(|| {
            failures
                .iter()
                .map(|failure| {
                    format!(
                        "{} profile `{}`: {}",
                        failure.backend, failure.profile, failure.error
                    )
                })
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    fn into_complete_rows(
        self,
    ) -> Result<Vec<RunningComposeContainerProfiled>, ContainerExecError> {
        let Some(summary) = self.failure_summary() else {
            return Ok(self.rows);
        };
        Err(ContainerExecError::Failure {
            command: "container runtime inventory".to_owned(),
            code: None,
            stdout: format!("collected {} container rows", self.rows.len()),
            stderr: format!("inventory is incomplete: {summary}"),
        })
    }
}

pub fn capture_compose_ps(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<String, ContainerExecError> {
    let output = run_docker_capture(repo_root, policy, args, label)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn shutdown_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    for (args, label) in shutdown_compose_commands(policy) {
        let (program, invocation_args) = compose_invocation_for_repo(repo_root, policy, &args);
        let rendered_args = invocation_args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        run_command_capture_with_timeout_and_env(
            repo_root,
            program,
            &rendered_args
                .iter()
                .map(|value| value.as_str())
                .collect::<Vec<_>>(),
            label,
            CONTAINER_SHUTDOWN_TIMEOUT,
            &[],
        )?;
    }
    Ok(())
}

pub fn run_docker_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_capture(repo_root, policy, args, label)
}

/// Ask the runtime for a restart policy on these containers, named by verified
/// full ID only. The caller proves ownership first (`restart::apply_after_start`).
/// This is a runtime
/// configuration change on the containers Compose created, not a start of
/// anything, so it cannot race Compose's own create/start (effigy#383).
pub fn set_restart_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    containers: &[String],
    restart: &str,
) -> Result<(), ContainerExecError> {
    let mut args = vec![
        OsString::from("update"),
        OsString::from("--restart"),
        OsString::from(restart),
    ];
    args.extend(containers.iter().map(OsString::from));
    capture_runtime_command(
        repo_root,
        policy,
        &args,
        "runtime update",
        Some(CONTAINER_SHUTDOWN_TIMEOUT),
        None,
    )
    .map(|_| ())
}

pub fn run_compose_capture(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_capture_with_env(repo_root, policy, args, label, &[])
}

/// [`run_compose_capture`] with the environment the one apply delivers. A
/// task's environment ensure and `container up` both use it (effigy#371).
pub fn run_compose_capture_with_env(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let (program, args) = compose_invocation_for_repo(repo_root, policy, args);
    run_compose_invocation_capture_with_env(repo_root, OsStr::new(program), &args, label, env)
}

pub fn list_running_compose_containers_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), false),
        "runtime ps",
        None,
        None,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// One container of a shared service's own Compose project, with the Compose
/// service and render identity the container carries. A shared project belongs
/// to no checkout, so a data command selects a container by these labels rather
/// than by a name pattern (effigy#477).
pub struct SharedProjectContainer {
    pub container_name: String,
    pub running: bool,
    /// `None` when no Effigy wrote the render label, which is what a container
    /// an older Effigy created looks like (effigy#377).
    pub render: Option<String>,
    /// The Compose service label, which is how a shared data target proves the
    /// container belongs to the declared service rather than a lookalike.
    pub service: Option<String>,
}

const SHARED_PROJECT_PS_FORMAT: &str = "{{.Names}}\t{{.Status}}\t{{.Label \"com.effigy.shared-render\"}}\t{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.service\"}}";

/// Every container of one shared-service Compose project, running or stopped,
/// with the render each was created from. A shared project belongs to no
/// checkout, so this reads the runtime by project name alone.
pub fn shared_project_containers(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Result<Vec<SharedProjectContainer>, ContainerExecError> {
    let args = vec![
        OsString::from("ps"),
        OsString::from("--all"),
        OsString::from("--filter"),
        OsString::from(format!("label=com.docker.compose.project={project_name}")),
        OsString::from("--format"),
        OsString::from(SHARED_PROJECT_PS_FORMAT),
    ];
    let output = capture_runtime_command(repo_root, policy, &args, "runtime ps --all", None, None)?;
    let refuse = || {
        ContainerExecError::refused(
            "runtime ps --all",
            "shared inventory is malformed or undecodable",
        )
    };
    std::str::from_utf8(&output.stdout)
        .map_err(|_| refuse())?
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            let [name, status, render, project, service] = fields.as_slice() else {
                return Err(refuse());
            };
            if name.trim().is_empty() || status.trim().is_empty() || *project != project_name {
                return Err(refuse());
            }
            Ok(SharedProjectContainer {
                container_name: name.trim().to_owned(),
                running: super::parse::compose_status_is_running(status),
                render: (!render.is_empty()).then(|| (*render).to_owned()),
                service: (!service.is_empty()).then(|| (*service).to_owned()),
            })
        })
        .collect()
}

/// Runtime `ps` arguments listing one Compose project's containers. The label
/// filter keeps the runtime from enumerating every container in a shared VM;
/// callers still filter the parsed rows by project as the authority.
fn project_ps_args(project_name: &str, include_stopped: bool) -> Vec<OsString> {
    let mut args = vec![OsString::from("ps")];
    if include_stopped {
        args.push(OsString::from("--all"));
    }
    args.extend([
        OsString::from("--filter"),
        OsString::from(format!("label=com.docker.compose.project={project_name}")),
        OsString::from("--format"),
        OsString::from(DOCKER_PS_FORMAT),
    ]);
    args
}

/// Deadline-aware compose `ps` probe. One caller-visible monotonic deadline is
/// converted to a per-spawn remainder, so a preceding liveness probe consumes
/// the same budget instead of granting this probe a fresh timeout. An expired
/// deadline never spawns; timeouts stay `unavailable` (distinct from stopped or
/// clean) and the bounded doctor path never triggers a Colima runtime repair.
pub fn list_running_compose_containers_for_policy_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), false),
        "runtime ps",
        None,
        deadline,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// Complete project inventory, including stopped rows, for a primary decision.
/// Unlike the general listing, incomplete labels and invalid UTF-8 refuse.
pub fn decision_project_inventory(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    selected: &crate::environment::BackendSelection,
    deadline: Instant,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let (program, args) = selected
        .compose_invocation(&project_ps_args(&policy.project_name, true))
        .map_err(|error| ContainerExecError::Failure {
            command: "runtime ps decision inventory".into(),
            code: None,
            stdout: String::new(),
            stderr: error.to_string(),
        })?;
    let output = run_backend_invocation_until(
        repo_root,
        &program,
        &args,
        "runtime ps decision inventory",
        &[],
        deadline,
    )?;
    let text = std::str::from_utf8(&output.stdout).map_err(|_| ContainerExecError::Failure {
        command: "runtime ps decision inventory".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "inventory is not UTF-8".to_owned(),
    })?;
    super::parse::strict_project_inventory(text, &policy.project_name)
}

/// Deadline-aware compose `ps` probe for an arbitrary Compose project in
/// this checkout — used by the `data_from` handover to discover a peer
/// environment's running services. The policy only selects the runtime
/// profile and backend; container selection is by the given project name.
pub fn list_running_compose_containers_for_project_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    deadline: Option<Instant>,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(project_name, false),
        "runtime ps",
        None,
        deadline,
    )?;

    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(project_name))
            .collect(),
    )
}

/// Deadline-aware compose `ps --all` probe that keeps stopped rows. A
/// project-wide `compose up` can start or recreate a stopped declared sibling,
/// so ownership must be proven for those rows before it is authorized, not
/// only for the running ones. A caller-visible monotonic deadline bounds the
/// single spawn and an expired deadline never spawns.
pub fn list_compose_containers_for_policy_including_stopped_with_deadline(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = capture_runtime_command(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), true),
        "runtime ps --all",
        None,
        deadline,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

pub fn list_running_compose_containers_for_policy_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), false),
        "runtime ps",
        timeout,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// Re-admit inventory rows from exact current inspect identities and bindings.
/// A ps name, reserved address, or successful command alone is not port proof.
/// Read the Compose labels an owned binding check needs. Docker compose
/// records the project working dir and an explicit `oneoff`; nerdctl compose
/// records neither, so the working dir falls back to Effigy's checkout label
/// and a missing `oneoff` is accepted (the inspected identity still rejects
/// one-off containers).
fn compose_binding_labels(labels: Option<&serde_json::Value>) -> (Option<&str>, bool) {
    let label = |name: &str| {
        labels
            .and_then(|labels| labels.get(name))
            .and_then(|value| value.as_str())
    };
    let working_dir = checkout_label(
        label("com.docker.compose.project.working_dir"),
        label(crate::runtime::scope::CHECKOUT_LABEL),
        label(crate::runtime::scope::SCOPE_LABEL),
    );
    let oneoff_ok = label("com.docker.compose.oneoff")
        .is_none_or(|value| matches!(value.to_ascii_lowercase().as_str(), "false" | "0" | "no"));
    (working_dir, oneoff_ok)
}

// A scoped generation's Compose working directory is the immutable render
// directory. The explicit checkout label and matching scope locate its owner.
// Unscoped legacy containers keep the historical working-directory rule.
fn checkout_label<'a>(
    working_dir: Option<&'a str>,
    checkout: Option<&'a str>,
    scope: Option<&str>,
) -> Option<&'a str> {
    if scope.is_some() {
        checkout.or(working_dir)
    } else {
        working_dir.or(checkout)
    }
}

#[cfg(test)]
mod compose_binding_label_tests {
    use super::compose_binding_labels;
    use serde_json::json;

    #[test]
    fn docker_labels_use_project_working_dir() {
        let labels = json!({
            "com.docker.compose.project.working_dir": "/repo",
            "com.effigy.checkout": "/elsewhere",
            "com.docker.compose.oneoff": "False",
        });
        assert_eq!(compose_binding_labels(Some(&labels)), (Some("/repo"), true));
    }

    #[test]
    fn scoped_render_directory_does_not_replace_the_verified_checkout_label() {
        let mut labels = json!({
            "com.docker.compose.project.working_dir": "/repo/.effigy/state/objects/hash/checkout/.effigy/runtime/compose",
            "com.effigy.checkout": "/repo",
            "com.effigy.scope": "owned-scope",
            "com.docker.compose.oneoff": "False",
        });
        assert_eq!(compose_binding_labels(Some(&labels)), (Some("/repo"), true));
        labels["com.effigy.checkout"] = json!("/foreign");
        assert_eq!(
            compose_binding_labels(Some(&labels)),
            (Some("/foreign"), true)
        );
    }

    #[test]
    fn nerdctl_labels_fall_back_to_effigy_checkout() {
        let labels = json!({
            "com.docker.compose.project": "acme",
            "com.effigy.checkout": "/repo",
        });
        assert_eq!(compose_binding_labels(Some(&labels)), (Some("/repo"), true));
    }

    #[test]
    fn oneoff_and_unlabelled_containers_are_not_owned() {
        let oneoff = json!({"com.docker.compose.project.working_dir": "/repo", "com.docker.compose.oneoff": "True"});
        assert_eq!(
            compose_binding_labels(Some(&oneoff)),
            (Some("/repo"), false)
        );
        assert_eq!(compose_binding_labels(Some(&json!({}))), (None, true));
        assert_eq!(compose_binding_labels(None), (None, true));
    }
}

#[cfg(test)]
mod legacy_admission_tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::OwnershipContext;
    use crate::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerSecretDelivery,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };

    fn policy(project: &str) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: PathBuf::from("/repo"),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![],
            managed_volumes: vec![],
            borrowed_volumes: vec![],
            data_peers: Vec::new(),
            data_from: None,
            shared_services: vec![],
            project_name: project.to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            runtime_host_key: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            workspace_source: crate::artifact_source::WorkspaceSource::Checkout,
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: vec![],
        }
    }

    fn unscoped_context(checkout: &Path) -> OwnershipContext {
        OwnershipContext {
            declared: BTreeSet::from(["app".to_owned()]),
            canonical: checkout.to_path_buf(),
            scope_token: None,
            staged_render: None,
        }
    }

    #[test]
    fn shutdown_uses_full_ids_and_checkout_scope_labels_without_inspect() {
        let private = tempfile::tempdir().expect("private shutdown labels");
        let root = private.path().canonicalize().expect("canonical checkout");
        let mut context = unscoped_context(&root);
        let policy = policy("acme");
        let id = "a".repeat(64);
        for checkout_column in [
            format!("{}\t", root.display()),
            format!("\t{}", root.display()),
        ] {
            let row = format!("{id}\tacme\t{checkout_column}\t\n");
            context
                .verify_shutdown_listing(&policy, row.as_bytes())
                .expect("Docker working_dir or nerdctl checkout label");
        }
        context
            .verify_shutdown_listing(&policy, b"")
            .expect("already gone");
        for row in [
            format!("short\tacme\t{}\t\t\n", root.display()),
            format!("{id}\tforeign\t{}\t\t\n", root.display()),
            format!("{id}\tacme\t/unknown-checkout\t\t\n"),
            format!("{id}\tacme\t{}\t\tforeign-scope\n", root.display()),
        ] {
            assert!(
                context
                    .verify_shutdown_listing(&policy, row.as_bytes())
                    .is_err(),
                "{row}"
            );
        }
        context.scope_token = Some("owned-scope".to_owned());
        let rendered = format!(
            "{id}\tacme\t/immutable/render\t{}\towned-scope\n",
            root.display()
        );
        context
            .verify_shutdown_listing(&policy, rendered.as_bytes())
            .expect("scoped render uses checkout label");
        let foreign = format!("{id}\tacme\t{}\t/foreign\towned-scope\n", root.display());
        assert!(context
            .verify_shutdown_listing(&policy, foreign.as_bytes())
            .is_err());
        let row = format!("{id}\tacme\t{}\t\towned-scope\n", root.display());
        context
            .verify_shutdown_listing(&policy, row.as_bytes())
            .expect("matching scoped ownership");
    }

    /// The retained nerdctl inspect shape an operator's report captured for the
    /// private builder (effigy#710), with every identity value derived from a
    /// private fixture checkout: full 64-character id, `Name` without a leading
    /// slash, `State.Status`/`Running`/`Restarting`, the bind plus both cargo
    /// caches and an empty `NetworkSettings.Ports`. No live container, engine
    /// or VM is read.
    fn retained_nerdctl_inspect(
        project: &str,
        checkout: &Path,
        workspace_source: &Path,
    ) -> serde_json::Value {
        let checkout_label = checkout.display().to_string();
        let source = workspace_source.display().to_string();
        serde_json::json!({
            "Id": "80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce",
            "Created": "2026-11-02T21:25:31.41161783Z",
            "Path": "bash",
            "Args": ["-lc", "while true; do sleep 3600; done"],
            "State": {
                "Status": "running",
                "Running": true,
                "Paused": false,
                "Restarting": true,
                "Pid": 983,
                "ExitCode": 0,
                "Error": "",
                "StartedAt": "2026-11-08T11:29:18.538605086Z",
                "FinishedAt": "",
            },
            "Image": "sha256:2f8c5ff844de522282140bc51b634eb1da22df359e701197f0b0016416f41217",
            "Name": format!("{project}-builder-1"),
            "RestartCount": 0,
            "Driver": "runc",
            "Platform": "linux/arm64/v8",
            "Mounts": [
                {"Type": "bind", "Source": source, "Destination": "/workspace", "Mode": "", "RW": true, "Propagation": ""},
                {"Type": "volume", "Name": format!("{project}-cargo-registry"), "Source": format!("/var/lib/nerdctl/1935db59/volumes/default/{project}-cargo-registry/_data"), "Destination": "/root/.cargo/registry", "Driver": "local", "Mode": "", "RW": true, "Propagation": ""},
                {"Type": "volume", "Name": format!("{project}-cargo-git"), "Source": format!("/var/lib/nerdctl/1935db59/volumes/default/{project}-cargo-git/_data"), "Destination": "/root/.cargo/git", "Driver": "local", "Mode": "", "RW": true, "Propagation": ""},
            ],
            "Config": {
                "Hostname": "builder",
                "Image": format!("docker.io/library/{project}-builder:latest"),
                "WorkingDir": "/workspace",
                "Labels": {
                    "com.docker.compose.project": project,
                    "com.docker.compose.service": "builder",
                    "com.effigy.checkout": checkout_label,
                    "nerdctl/name": format!("{project}-builder-1"),
                    "nerdctl/platform": "linux/arm64/v8",
                    "nerdctl/state-dir": "/var/lib/nerdctl/1935db59/containers/default/80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce",
                    "containerd.io/restart.policy": "unless-stopped",
                    "containerd.io/restart.status": "running",
                },
            },
            // The live report: nerdctl's compatibility view rebuilds CapDrop from
            // the default capability set, so a dropped SYS_ADMIN is never listed.
            "HostConfig": {
                "Privileged": false,
                "CapAdd": [],
                "CapDrop": [],
            },
            "NetworkSettings": {
                "Bridge": "",
                "SandboxID": "0d5d1c0a8d9dbfc0f9d3d6f6d8ba44a44ed8d5b2f7b3f2f5d9f7c1a9e6d3b8f0",
                "HairpinMode": false,
                "Ports": {},
                "Gateway": "",
                "IPAddress": "10.4.2.3",
                "IPPrefixLen": 24,
                "MacAddress": "aa:4b:8a:7c:7e:6c",
            },
        })
    }

    fn builder_row(project: &str) -> super::RunningComposeContainer {
        super::RunningComposeContainer {
            container_name: format!("{project}-builder-1"),
            status: "Up".into(),
            ports: vec![],
            project_name: Some(project.to_owned()),
            working_dir: None,
            service: Some("builder".to_owned()),
            oneoff: false,
        }
    }

    fn builder_context(checkout: &Path) -> OwnershipContext {
        OwnershipContext {
            declared: BTreeSet::from(["builder".to_owned()]),
            canonical: checkout.to_path_buf(),
            scope_token: None,
            staged_render: None,
        }
    }

    #[test]
    fn retained_nerdctl_shape_proves_an_owned_current_builder() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let context = builder_context(&checkout);
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let binding = context
            .verify_inspected(
                &checkout,
                &policy(project),
                &builder_row(project),
                serde_json::json!([record]).to_string().as_bytes(),
            )
            .expect("the retained nerdctl shape is an owned running builder");
        assert_eq!(
            binding.id(),
            "80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce"
        );
        assert_eq!(binding.scope, None);
        assert_eq!(
            super::workspace_builder_source_mount(
                record.get("Mounts").and_then(|value| value.as_array())
            )
            .as_ref()
            .map(|(source, _)| source.as_path()),
            Some(checkout.as_path())
        );
        assert!(super::workspace_builder_has_no_published_ports(Some(
            &record
        )));
        assert!(super::builder_cache_volumes_are_owned(
            record.get("Mounts").and_then(|value| value.as_array()),
            project
        ));
        // Docker's witness cannot be met by the nerdctl compatibility shape...
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        // ...the native OCI spec of the same full ID is the witness that is.
        let id = record["Id"].as_str().expect("id");
        assert_eq!(
            super::native_capability_absent(
                Some(&record),
                &native_inspect(id, default_capabilities()),
                id
            ),
            Ok(true)
        );
    }

    /// The OCI process capability sets nerdctl stores for the builder. Compose
    /// `cap_drop: [SYS_ADMIN]` on a container never granted SYS_ADMIN leaves
    /// these sets at the default 14 capabilities.
    fn default_capabilities() -> serde_json::Value {
        let defaults = [
            "CAP_CHOWN",
            "CAP_DAC_OVERRIDE",
            "CAP_FSETID",
            "CAP_FOWNER",
            "CAP_MKNOD",
            "CAP_NET_RAW",
            "CAP_SETGID",
            "CAP_SETUID",
            "CAP_SETFCAP",
            "CAP_SETPCAP",
            "CAP_NET_BIND_SERVICE",
            "CAP_SYS_CHROOT",
            "CAP_KILL",
            "CAP_AUDIT_WRITE",
        ];
        serde_json::json!({
            "bounding": defaults,
            "effective": defaults,
            "permitted": defaults,
        })
    }

    /// One native inspect of `id`, as `nerdctl container inspect --mode=native`
    /// prints it: the containerd-native record with its OCI `Spec`.
    fn native_inspect(id: &str, capabilities: serde_json::Value) -> Vec<u8> {
        serde_json::json!([{
            "ID": id,
            "Spec": {"process": {"capabilities": capabilities}},
        }])
        .to_string()
        .into_bytes()
    }

    #[test]
    fn running_inspect_refuses_an_extra_host_bind_and_a_missing_sys_admin_drop() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["Mounts"]
            .as_array_mut()
            .expect("mounts")
            .push(serde_json::json!({
                "Type": "bind",
                "Source": "/opt/extra",
                "Destination": "/opt/extra",
                "RW": true
            }));
        assert!(!super::builder_cache_volumes_are_owned(
            record.get("Mounts").and_then(|value| value.as_array()),
            project
        ));
        // Docker's compatibility record names the drop; without it, refuse.
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["HostConfig"]["CapDrop"] = serde_json::json!(["SYS_ADMIN"]);
        assert!(super::builder_remount_capability_dropped(Some(&record)));
        record["HostConfig"]["CapDrop"] = serde_json::json!([]);
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        record["HostConfig"]["CapDrop"] = serde_json::json!(["SYS_ADMIN"]);
        record["HostConfig"]["Privileged"] = serde_json::json!(true);
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        record["HostConfig"]["Privileged"] = serde_json::json!(false);
        record["HostConfig"]["CapAdd"] = serde_json::json!(["SYS_PTRACE"]);
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        record.as_object_mut().expect("record").remove("HostConfig");
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
    }

    #[test]
    fn native_witness_refuses_sys_admin_in_any_process_capability_set() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id");
        // The exec path inherits the stored sets, so any one of them holding
        // SYS_ADMIN is the remount capability still being available.
        for set in [
            "bounding",
            "effective",
            "inheritable",
            "permitted",
            "ambient",
        ] {
            let mut capabilities = default_capabilities();
            capabilities[set] = serde_json::json!(["CAP_CHOWN", "CAP_SYS_ADMIN"]);
            assert_eq!(
                super::native_capability_absent(
                    Some(&record),
                    &native_inspect(id, capabilities.clone()),
                    id
                ),
                Ok(false),
                "{set} SYS_ADMIN"
            );
            // A known non-default capability outside bounding is power too.
            capabilities[set] = serde_json::json!(["CAP_CHOWN", "CAP_SYS_PTRACE"]);
            assert_eq!(
                super::native_capability_absent(
                    Some(&record),
                    &native_inspect(id, capabilities),
                    id
                ),
                Ok(false),
                "{set}"
            );
        }
    }

    #[test]
    fn native_witness_refuses_privilege_and_add_back() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let id = "80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce";
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["HostConfig"]["Privileged"] = serde_json::json!(true);
        assert_eq!(
            super::native_capability_absent(
                Some(&record),
                &native_inspect(id, default_capabilities()),
                id
            ),
            Ok(false)
        );
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["HostConfig"]["CapAdd"] = serde_json::json!(["SYS_PTRACE"]);
        assert_eq!(
            super::native_capability_absent(
                Some(&record),
                &native_inspect(id, default_capabilities()),
                id
            ),
            Ok(false)
        );
    }

    #[test]
    fn native_witness_refuses_missing_malformed_or_replaced_identity() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id");
        let other = "b".repeat(64);
        let reject = |output: Vec<u8>, case: &str| {
            assert!(
                super::native_capability_absent(Some(&record), &output, id).is_err(),
                "{case} must refuse"
            );
        };
        reject(
            native_inspect(&other, default_capabilities()),
            "replaced id",
        );
        reject(b"not json".to_vec(), "non-JSON");
        reject(b"[]".to_vec(), "no record");
        reject(
            serde_json::json!([{"ID": id}]).to_string().into_bytes(),
            "no Spec",
        );
        reject(
            native_inspect(id, serde_json::json!({"effective": ["CAP_CHOWN"]})),
            "no bounding set",
        );
        reject(
            native_inspect(id, serde_json::json!(["CAP_CHOWN"])),
            "capabilities is a list",
        );
        let mut scalar = default_capabilities();
        scalar["effective"] = serde_json::json!("CAP_CHOWN");
        reject(native_inspect(id, scalar), "scalar set");
        for entry in [
            "SYS_ADMIN",
            "cap_sys_admin",
            "CAP_Sys_Admin",
            "",
            "CAP_SYS_ADMINX",
            "CAP_BOGUS",
        ] {
            let mut capabilities = default_capabilities();
            capabilities["bounding"] = serde_json::json!(["CAP_CHOWN", entry]);
            reject(native_inspect(id, capabilities), entry);
        }
    }

    #[test]
    fn retained_nerdctl_shape_refuses_another_checkouts_source_without_adoption() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        // The retained builder effigy#710 collided with: the same project
        // spelling, a different checkout's real source and no mount of this
        // checkout. The foreign source is a derived private directory, never
        // the retained checkout itself.
        let foreign = private.path().join("other-checkout");
        std::fs::create_dir_all(&foreign).expect("foreign checkout");
        let foreign = foreign.canonicalize().expect("canonical foreign checkout");
        let project = "effigy-artifact-builder-v1";
        let context = builder_context(&checkout);
        let record = retained_nerdctl_inspect(project, &foreign, &foreign);
        let error = context
            .verify_inspected(
                &checkout,
                &policy(project),
                &builder_row(project),
                serde_json::json!([record]).to_string().as_bytes(),
            )
            .expect_err("a builder of another checkout's source is never adopted");
        assert!(
            error.to_string().contains("does not match owned project"),
            "{error}"
        );
        assert_eq!(
            super::workspace_builder_source_mount(
                record.get("Mounts").and_then(|value| value.as_array())
            )
            .as_ref()
            .map(|(source, _)| source.as_path()),
            Some(foreign.as_path())
        );
    }

    #[test]
    fn native_witness_refuses_extra_records_and_unknown_capability_sets() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id");
        // A second record for the same ID is not the one the decision read.
        let one = native_inspect(id, default_capabilities());
        let mut two: Vec<serde_json::Value> =
            serde_json::from_slice(&one).expect("one native record");
        two.push(two[0].clone());
        assert!(super::native_capability_absent(
            Some(&record),
            &serde_json::to_vec(&two).expect("two records"),
            id
        )
        .is_err());
        // An unknown set beside the five OCI sets may hold SYS_ADMIN and must
        // not be ignored.
        for key in ["ambientExtra", "Bounding", "bounding_extra"] {
            let mut capabilities = default_capabilities();
            capabilities[key] = serde_json::json!(["CAP_SYS_ADMIN"]);
            assert!(
                super::native_capability_absent(
                    Some(&record),
                    &native_inspect(id, capabilities),
                    id
                )
                .is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn nerdctl_compat_add_back_must_be_present_and_strict() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id").to_owned();
        let output = native_inspect(&id, default_capabilities());
        // Missing or malformed compatibility fields are refused, never read as
        // an add-back-free builder that recovery could replace.
        type Mutation = fn(&mut serde_json::Value);
        let malformed: [(&str, Mutation); 5] = [
            ("CapAdd missing", |row| {
                row["HostConfig"]
                    .as_object_mut()
                    .expect("host")
                    .remove("CapAdd");
            }),
            ("CapAdd null", |row| {
                row["HostConfig"]["CapAdd"] = serde_json::Value::Null
            }),
            ("CapAdd string", |row| {
                row["HostConfig"]["CapAdd"] = serde_json::json!("CAP_SYS_PTRACE")
            }),
            ("Privileged missing", |row| {
                row["HostConfig"]
                    .as_object_mut()
                    .expect("host")
                    .remove("Privileged");
            }),
            ("HostConfig missing", |row| {
                row.as_object_mut().expect("record").remove("HostConfig");
            }),
        ];
        for (case, mutate) in malformed {
            let mut record = record.clone();
            mutate(&mut record);
            assert!(
                super::native_capability_absent(Some(&record), &output, &id).is_err(),
                "{case} must refuse"
            );
        }
        // Well-formed and present: an empty list is the witness; a privileged
        // builder or an added capability is a definite Ok(false), not an error.
        assert_eq!(
            super::native_capability_absent(Some(&record), &output, &id),
            Ok(true)
        );
    }

    #[test]
    fn inspect_refusals_name_the_concrete_cause_and_never_claim_a_signal() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let context = builder_context(&checkout);
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record
            .as_object_mut()
            .expect("record")
            .get_mut("State")
            .and_then(|state| state.as_object_mut())
            .expect("state")
            .remove("Status");
        let error = context
            .verify_inspected(
                &checkout,
                &policy(project),
                &builder_row(project),
                serde_json::json!([record]).to_string().as_bytes(),
            )
            .expect_err("an incomplete inspect record is not proof of identity");
        assert_eq!(
            error.to_string(),
            "inspect owned binding refused: binding unavailable: canonical container identity is unverified: inspect record missing State.Status"
        );
    }

    #[test]
    fn builder_decoder_refuses_identity_source_and_created_ownership_permutations() {
        use std::os::unix::fs::MetadataExt;
        let private = tempfile::tempdir().expect("checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-builder-table";
        let mut context = builder_context(&checkout);
        context.scope_token = Some("owned-scope".to_owned());
        let metadata = std::fs::metadata(&checkout).expect("checkout identity");
        for created in [false, true] {
            let mut base = retained_nerdctl_inspect(project, &checkout, &checkout);
            base["Mounts"][0]["RW"] = serde_json::json!(false);
            base["Config"]["Labels"]["com.effigy.scope"] = serde_json::json!("owned-scope");
            if created {
                base["State"] = serde_json::json!({"Status":"", "Running":false, "Paused":false, "Restarting":false, "Pid":0, "StartedAt":"", "FinishedAt":"", "Error":""});
                for name in [
                    super::CHECKOUT_GENERATION_LABEL,
                    super::CHECKOUT_FENCE_LABEL,
                ] {
                    base["Config"]["Labels"][name] = serde_json::json!(
                        serde_json::json!({"device":metadata.dev(),"inode":metadata.ino()})
                            .to_string()
                    );
                }
            }
            let mut row = builder_row(project);
            if created {
                row.status = "Created".into();
            }
            let decode = |record: serde_json::Value| {
                super::decode_workspace_builder(
                    &context,
                    &policy(project),
                    &row,
                    &serde_json::json!([record]),
                    crate::compose::ComposeBackend::ColimaNerdctl,
                )
            };
            assert!(decode(base.clone()).is_ok(), "baseline created={created}");
            for case in [
                "scope",
                "missing_scope",
                "project",
                "service",
                "oneoff",
                "checkout",
                "unlabelled",
                "working_dir",
                "short_id",
                "name",
                "ports",
                "missing_bind",
                "shadow_bind",
                "foreign_bind",
            ] {
                let mut record = base.clone();
                match case {
                    "scope" => record["Config"]["Labels"]["com.effigy.scope"] = serde_json::json!("foreign"),
                    "missing_scope" => { record["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.scope"); }
                    "project" => record["Config"]["Labels"]["com.docker.compose.project"] = serde_json::json!("foreign"),
                    "service" => record["Config"]["Labels"]["com.docker.compose.service"] = serde_json::json!("app"),
                    "oneoff" => record["Config"]["Labels"]["com.docker.compose.oneoff"] = serde_json::json!("True"),
                    "checkout" => record["Config"]["Labels"]["com.effigy.checkout"] = serde_json::json!("/nonexistent/foreign"),
                    "unlabelled" => { record["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.checkout"); }
                    "working_dir" => record["Config"]["Labels"]["com.docker.compose.project.working_dir"] = serde_json::json!("/nonexistent/foreign"),
                    "short_id" => record["Id"] = serde_json::json!("a".repeat(12)),
                    "name" => record["Name"] = serde_json::json!("successor-builder-1"),
                    "ports" => record["NetworkSettings"]["Ports"] = serde_json::json!({"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"12345"}]}),
                    "missing_bind" => record["Mounts"] = serde_json::json!([]),
                    "shadow_bind" => record["Mounts"].as_array_mut().unwrap().push(serde_json::json!({"Type":"bind", "Source":checkout, "Destination":"/workspace/src", "RW":true})),
                    "foreign_bind" => record["Mounts"][0]["Source"] = serde_json::json!("/nonexistent/foreign"),
                    _ => unreachable!(),
                }
                assert!(decode(record).is_err(), "created={created}: {case}");
            }
            if created {
                for case in [
                    "generation",
                    "fence",
                    "missing_generation",
                    "cache_name",
                    "extra_mount",
                ] {
                    let mut record = base.clone();
                    match case {
                        "generation" => record["Config"]["Labels"][super::CHECKOUT_GENERATION_LABEL] = serde_json::json!(serde_json::json!({"device":metadata.dev(),"inode":metadata.ino()+1}).to_string()),
                        "fence" => record["Config"]["Labels"][super::CHECKOUT_FENCE_LABEL] = serde_json::json!(serde_json::json!({"device":metadata.dev()+1,"inode":metadata.ino()}).to_string()),
                        "missing_generation" => { record["Config"]["Labels"].as_object_mut().unwrap().remove(super::CHECKOUT_GENERATION_LABEL); }
                        "cache_name" => record["Mounts"][1]["Name"] = serde_json::json!("foreign-cache"),
                        "extra_mount" => record["Mounts"].as_array_mut().unwrap().push(serde_json::json!({"Type":"volume", "Name":"extra", "Destination":"/extra", "RW":true})),
                        _ => unreachable!(),
                    }
                    assert!(decode(record).is_err(), "created: {case}");
                }
            }
        }
    }

    #[test]
    fn builder_decoder_refuses_non_builder_listing_even_if_declared() {
        let private = tempfile::tempdir().expect("checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-builder-table";
        let mut context = builder_context(&checkout);
        context.declared.insert("app".to_owned());
        for (service, oneoff) in [(Some("app"), false), (None, false), (Some("builder"), true)] {
            let mut row = builder_row(project);
            row.service = service.map(str::to_owned);
            row.oneoff = oneoff;
            let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
            record["Config"]["Labels"]["com.docker.compose.service"] = serde_json::json!(service);
            assert!(
                super::decode_workspace_builder(
                    &context,
                    &policy(project),
                    &row,
                    &serde_json::json!([record]),
                    crate::compose::ComposeBackend::ColimaNerdctl,
                )
                .is_err(),
                "service={service:?}, oneoff={oneoff}"
            );
        }
    }

    #[test]
    fn builder_preparation_table_keeps_state_and_recovery_boundaries() {
        use super::{WorkspaceBuilderPreparation as Step, WorkspaceBuilderState as State};
        let private = tempfile::tempdir().expect("checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-builder-table";
        let context = builder_context(&checkout);
        for no_change in [false, true] {
            for retained in [false, true] {
                let result = super::builder_preparation(None, retained, no_change);
                assert_eq!(
                    matches!(result, Ok(Step::Activate(None))),
                    !retained && !no_change
                );
            }
            for state in [
                State::NeverStarted,
                State::Stopped,
                State::Running,
                State::Unknown,
            ] {
                for defect in [
                    "none",
                    "writable",
                    "mounts",
                    "capabilities",
                    "old_source",
                    "foreign_source",
                ] {
                    let record = retained_nerdctl_inspect(project, &checkout, &checkout);
                    let mut inspected = super::decode_workspace_builder(
                        &context,
                        &policy(project),
                        &builder_row(project),
                        &serde_json::json!([record]),
                        crate::compose::ComposeBackend::ColimaNerdctl,
                    )
                    .expect("owned record");
                    inspected.state = state;
                    inspected.observed.never_started = matches!(state, State::NeverStarted);
                    inspected.observed.read_only = defect != "writable";
                    inspected.observed.mounts_owned = defect != "mounts";
                    inspected.observed.remount_capability_dropped = defect != "capabilities";
                    inspected.source_owned = defect != "foreign_source";
                    inspected.matching = defect == "none";
                    let result = super::builder_preparation(Some(inspected), false, no_change);
                    let reuse = matches!(state, State::Running) && defect == "none";
                    let activate = matches!(state, State::Running | State::NeverStarted)
                        && defect != "none"
                        && defect != "foreign_source"
                        && !no_change;
                    assert_eq!(
                        matches!(&result, Ok(Step::Reuse(_))),
                        reuse,
                        "reuse {defect}"
                    );
                    assert_eq!(
                        matches!(&result, Ok(Step::Activate(Some(_)))),
                        activate,
                        "activate {defect}"
                    );
                    assert_eq!(result.is_err(), !reuse && !activate, "refusal {defect}");
                }
            }
        }
    }
}

/// An inspected binding. Fields are private so only strict inspection can mint it.
/// Reverification is still required before an effect; this is not a remote lock.
#[derive(Debug, Clone)]
pub struct VerifiedComposeContainer {
    row: RunningComposeContainer,
    id: String,
    checkout: std::path::PathBuf,
    scope: Option<String>,
    profile: String,
    backend: crate::compose::ComposeBackend,
    // Preserve the strict builder witness, not an expired earlier probe budget.
    // Every reinspection uses its current caller's unchanged deadline.
    workspace_artifact_builder: bool,
}

impl VerifiedComposeContainer {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn service(&self) -> Option<&str> {
        self.row.service.as_deref()
    }

    pub fn reverify(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        deadline: Instant,
    ) -> Result<(), ContainerExecError> {
        let current = if self.workspace_artifact_builder {
            vec![verify_workspace_artifact_builder(
                repo_root, policy, &self.row, deadline,
            )?]
        } else {
            verify_owned_running_compose_identities(
                repo_root,
                policy,
                std::slice::from_ref(&self.row),
                deadline,
            )?
        };
        if current.len() == 1
            && current[0].id == self.id
            && current[0].checkout == self.checkout
            && current[0].scope == self.scope
            && current[0].profile == self.profile
            && current[0].backend == self.backend
        {
            return Ok(());
        }
        Err(ContainerExecError::refused(
            "inspect owned binding",
            "binding unavailable: container full ID or checkout/scope/backend binding changed before effect",
        ))
    }
}

/// Refuse a builder declaration that could activate any additional service.
pub fn verify_workspace_artifact_builder_declaration(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    for file in &policy.compose_files {
        let content = policy.read_compose(file).map_err(|error| {
            ContainerExecError::refused("read workspace builder declaration", error.to_string())
        })?;
        let compose: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerExecError::refused("parse workspace builder declaration", error.to_string())
        })?;
        let builder = compose
            .get("services")
            .and_then(|services| services.get("builder"));
        if compose.get("include").is_some()
            || builder.is_some_and(|builder| {
                builder.get("extends").is_some()
                    || builder.get("depends_on").is_some()
                    || builder.get("ports").is_some()
            })
        {
            return Err(ContainerExecError::refused(
                "verify workspace builder declaration",
                "workspace artifact builder cannot include, extend, depend on or publish other workloads",
            ));
        }
    }
    let services = declared_service_names(policy, &policy.compose_files)?;
    if services.len() != 1 || !services.contains("builder") {
        return Err(ContainerExecError::refused(
            "verify workspace builder declaration",
            "workspace artifact builder must declare only the builder service",
        ));
    }
    Ok(())
}

/// Strict builder execution authority: explicit checkout/scope labels, a full
/// immutable identity and a read-only source bind at /workspace. Unlike stack
/// reactivation this never adopts legacy unlabelled containers.
pub fn verify_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<VerifiedComposeContainer, ContainerExecError> {
    let inspected = inspect_workspace_artifact_builder(repo_root, policy, row, deadline)?;
    match builder_preparation(Some(inspected), false, false)? {
        WorkspaceBuilderPreparation::Reuse(binding) => Ok(*binding),
        WorkspaceBuilderPreparation::Activate(_) => Err(ContainerExecError::refused(
            "inspect workspace builder",
            "workspace builder requires preparation; no execution binding granted",
        )),
    }
}

/// An owned builder's immutable identity and source, rechecked before recovery.
/// It grants no execution or mutation authority by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWorkspaceArtifactBuilder {
    pub id: String,
    pub source: PathBuf,
    pub never_started: bool,
    pub read_only: bool,
    pub mounts_owned: bool,
    pub remount_capability_dropped: bool,
}

/// One preparation step. Activation still goes through the maintained lifecycle
/// and rechecks the exact observed target immediately before it mutates anything.
#[derive(Debug)]
pub enum WorkspaceBuilderPreparation {
    Reuse(Box<VerifiedComposeContainer>),
    Activate(Option<ObservedWorkspaceArtifactBuilder>),
}

pub fn workspace_artifact_builder_preparation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    deadline: Instant,
    retained: bool,
    no_change: bool,
) -> Result<WorkspaceBuilderPreparation, ContainerExecError> {
    let inspected = match rows {
        [] => None,
        [row] => {
            Some(inspect_workspace_artifact_builder(repo_root, policy, row, deadline)?)
        }
        _ => return Err(ContainerExecError::refused(
            "inspect workspace builder",
            "workspace artifact builder inventory must contain exactly one owned builder; no build attempted",
        )),
    };
    builder_preparation(inspected, retained, no_change)
}

/// Choose from already-decoded facts; this function performs no runtime effects.
fn builder_preparation(
    inspected: Option<InspectedWorkspaceBuilder>,
    retained: bool,
    no_change: bool,
) -> Result<WorkspaceBuilderPreparation, ContainerExecError> {
    let refuse = |message| ContainerExecError::refused("inspect workspace builder", message);
    let Some(inspected) = inspected else {
        if retained {
            return Err(refuse("workspace artifact builder record has no matching container; retained state was not removed or adopted"));
        }
        if no_change {
            return Err(refuse("no-change workspace artifact build refuses to create the builder container; the profile must already hold it"));
        }
        return Ok(WorkspaceBuilderPreparation::Activate(None));
    };
    match inspected.state {
        WorkspaceBuilderState::Stopped => {
            return Err(refuse(
                "workspace builder is stopped; no running execution binding",
            ))
        }
        WorkspaceBuilderState::Unknown => {
            return Err(refuse(
                "workspace builder state is unknown; no running execution binding",
            ))
        }
        WorkspaceBuilderState::Running | WorkspaceBuilderState::NeverStarted => {}
    }
    let observed = inspected.observed;
    if !observed.never_started && inspected.matching {
        return Ok(WorkspaceBuilderPreparation::Reuse(Box::new(
            inspected.binding,
        )));
    }
    if !inspected.source_owned {
        return Err(refuse("workspace builder source bind is not this checkout's own; its source bind is not adopted"));
    }
    // A matching never-started container used to fail the strict running check
    // and was refused. Preserve that boundary instead of granting start authority.
    if observed.never_started && inspected.matching {
        return Err(refuse(
            "workspace builder is never-started; no running execution binding",
        ));
    }
    if no_change {
        return Err(refuse("no-change workspace artifact build refuses an old, writable or mismatched owned builder before claim; it cannot create, replace or reconfigure it"));
    }
    Ok(WorkspaceBuilderPreparation::Activate(Some(observed)))
}

pub fn observed_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<ObservedWorkspaceArtifactBuilder, ContainerExecError> {
    let inspected = inspect_workspace_artifact_builder(repo_root, policy, row, deadline)?;
    if matches!(
        inspected.state,
        WorkspaceBuilderState::Stopped | WorkspaceBuilderState::Unknown
    ) {
        return Err(ContainerExecError::refused(
            "inspect workspace builder",
            "workspace builder is not running or never-started",
        ));
    }
    Ok(inspected.observed)
}

const CHECKOUT_GENERATION_LABEL: &str = "com.effigy.checkout-generation";
const CHECKOUT_FENCE_LABEL: &str = "com.effigy.checkout-fence";
const BUILDER_CACHE_VOLUMES: [(&str, &str); 2] = [
    ("/root/.cargo/registry", "cargo-registry"),
    ("/root/.cargo/git", "cargo-git"),
];

/// Exactly the builder's two cargo caches, each the named volume of this
/// project, read-write, with nothing else mounted beside the `/workspace` bind.
fn builder_cache_volumes_are_owned(
    mounts: Option<&Vec<serde_json::Value>>,
    project_name: &str,
) -> bool {
    let Some(mounts) = mounts else { return false };
    mounts.len() == BUILDER_CACHE_VOLUMES.len() + 1
        && BUILDER_CACHE_VOLUMES.iter().all(|(destination, suffix)| {
            let name = format!("{project_name}-{suffix}");
            mounts.iter().any(|mount| {
                mount.get("Type").and_then(|value| value.as_str()) == Some("volume")
                    && mount.get("Destination").and_then(|value| value.as_str())
                        == Some(destination)
                    && mount.get("Name").and_then(|value| value.as_str()) == Some(name.as_str())
                    && mount.get("RW").and_then(|value| value.as_bool()) == Some(true)
            })
        })
}

/// Whether the compatibility record is unprivileged and adds nothing back.
/// A missing HostConfig is not proof of either.
fn host_is_unprivileged_without_add_back(record: Option<&serde_json::Value>) -> bool {
    let Some(host) = record.and_then(|record| record.get("HostConfig")) else {
        return false;
    };
    if host.get("Privileged").and_then(|value| value.as_bool()) != Some(false) {
        return false;
    }
    match host.get("CapAdd") {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::Array(caps)) => caps.is_empty(),
        _ => false,
    }
}

/// Docker's witness: HostConfig CapDrop names SYS_ADMIN, nothing is added back,
/// and the container is not privileged. Docker lists the names it was asked to
/// drop, so a missing HostConfig is not proof of the drop.
///
/// nerdctl's compatibility inspect cannot report this: it rebuilds CapDrop from
/// the default capability set only, and SYS_ADMIN is not in that set. Colima
/// nerdctl therefore uses [`remount_capability_witness`] instead.
fn builder_remount_capability_dropped(record: Option<&serde_json::Value>) -> bool {
    if !host_is_unprivileged_without_add_back(record) {
        return false;
    }
    record
        .and_then(|record| record.pointer("/HostConfig/CapDrop"))
        .and_then(|value| value.as_array())
        .is_some_and(|caps| {
            caps.iter().any(|cap| {
                cap.as_str().is_some_and(|name| {
                    name.eq_ignore_ascii_case("SYS_ADMIN")
                        || name.eq_ignore_ascii_case("CAP_SYS_ADMIN")
                })
            })
        })
}

/// Whether the exact owned builder proves the remount capability is gone on the
/// backend that runs it.
///
/// On Colima nerdctl the witness is the stored OCI spec of the same full ID. Its
/// process capability sets are what the builder runs with, and `nerdctl exec`
/// copies that stored `Process` (only `--privileged` rewrites the sets), so the
/// restriction the exec path inherits is the one observed here. Compose
/// `cap_drop` reaches the spec through `WithDroppedCapabilities`, which removes
/// the name from bounding, effective, permitted and inheritable.
///
/// A missing, malformed, or other-identity native record is an error. A complete
/// record that holds any process capability beyond containerd's default set
/// (CAP_SYS_ADMIN or other power) is `Ok(false)`.
fn remount_capability_witness(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    id: &str,
    record: Option<&serde_json::Value>,
    deadline: Instant,
) -> Result<bool, ContainerExecError> {
    const CONTEXT: &str = "inspect workspace builder";
    if !matches!(
        crate::compose::resolve_compose_backend_for_repo(repo_root, policy),
        crate::compose::ComposeBackend::ColimaNerdctl
    ) {
        return Ok(builder_remount_capability_dropped(record));
    }
    let remaining = super::process::remaining_until_deadline(deadline, CONTEXT)?;
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &inspect_native_spec_args(id),
        CONTEXT,
        deadline,
        remaining,
    )?;
    native_capability_absent(record, &output.stdout, id)
        .map_err(|reason| ContainerExecError::refused(CONTEXT, reason))
}

/// nerdctl builders run with containerd's default unix capabilities
/// (`defaultUnixCaps` in containerd v2.3.1 `pkg/oci/spec.go`, sha256
/// 7029327e775cf5bb1bfc632b45ecfdb63d7872201b973edf27e88866f907276c). Bounding,
/// effective and permitted default to this set; inheritable and ambient default
/// to empty. Any process capability beyond it is power added back.
const NERDCTL_DEFAULT_CAPS: [&str; 14] = [
    "CAP_CHOWN",
    "CAP_DAC_OVERRIDE",
    "CAP_FSETID",
    "CAP_FOWNER",
    "CAP_MKNOD",
    "CAP_NET_RAW",
    "CAP_SETGID",
    "CAP_SETUID",
    "CAP_SETFCAP",
    "CAP_SETPCAP",
    "CAP_NET_BIND_SERVICE",
    "CAP_SYS_CHROOT",
    "CAP_KILL",
    "CAP_AUDIT_WRITE",
];

/// Every capability name the kernel's uapi header defines
/// (`include/uapi/linux/capability.h`, v6.12, sha256
/// ee832adca0d6db0866ddbf1dd3426c5a0df63901bf255bde77e4d8305cf5b5e0). A name
/// outside this list is unknown capability data, not a harmless extra.
const LINUX_CAPABILITY_NAMES: [&str; 41] = [
    "CAP_CHOWN",
    "CAP_DAC_OVERRIDE",
    "CAP_DAC_READ_SEARCH",
    "CAP_FOWNER",
    "CAP_FSETID",
    "CAP_KILL",
    "CAP_SETGID",
    "CAP_SETUID",
    "CAP_SETPCAP",
    "CAP_LINUX_IMMUTABLE",
    "CAP_NET_BIND_SERVICE",
    "CAP_NET_BROADCAST",
    "CAP_NET_ADMIN",
    "CAP_NET_RAW",
    "CAP_IPC_LOCK",
    "CAP_IPC_OWNER",
    "CAP_SYS_MODULE",
    "CAP_SYS_RAWIO",
    "CAP_SYS_CHROOT",
    "CAP_SYS_PTRACE",
    "CAP_SYS_PACCT",
    "CAP_SYS_ADMIN",
    "CAP_SYS_BOOT",
    "CAP_SYS_NICE",
    "CAP_SYS_RESOURCE",
    "CAP_SYS_TIME",
    "CAP_SYS_TTY_CONFIG",
    "CAP_MKNOD",
    "CAP_LEASE",
    "CAP_AUDIT_WRITE",
    "CAP_AUDIT_CONTROL",
    "CAP_SETFCAP",
    "CAP_MAC_OVERRIDE",
    "CAP_MAC_ADMIN",
    "CAP_SYSLOG",
    "CAP_WAKE_ALARM",
    "CAP_BLOCK_SUSPEND",
    "CAP_AUDIT_READ",
    "CAP_PERFMON",
    "CAP_BPF",
    "CAP_CHECKPOINT_RESTORE",
];

/// Decide the nerdctl witness from one native inspect of the full `id`.
///
/// OCI omits an empty capability set (`omitempty`), so an absent set reads as
/// empty. An absent `bounding` set, a missing `process` or `capabilities`
/// object, a non-list set or any entry that is not an upper-case `CAP_` name is
/// malformed. The compatibility record must still be unprivileged with nothing
/// added back.
fn native_capability_absent(
    record: Option<&serde_json::Value>,
    native_output: &[u8],
    id: &str,
) -> Result<bool, String> {
    let raw: serde_json::Value = serde_json::from_slice(native_output)
        .map_err(|error| format!("native inspect output is not JSON: {error}"))?;
    let records = raw
        .as_array()
        .ok_or_else(|| "native inspect output is not a record list".to_owned())?;
    // One record for one ID: a second record is data this decision never saw.
    let [native] = records.as_slice() else {
        return Err(format!(
            "native inspect returned {} records; exactly one is required",
            records.len()
        ));
    };
    if native.get("ID").and_then(|value| value.as_str()) != Some(id) {
        return Err(
            "native inspect reports a different full container ID than the verified binding"
                .to_owned(),
        );
    }
    let capabilities = native
        .pointer("/Spec/process/capabilities")
        .and_then(|value| value.as_object())
        .ok_or_else(|| "native inspect has no Spec.process.capabilities".to_owned())?;
    const SETS: [&str; 5] = [
        "bounding",
        "effective",
        "inheritable",
        "permitted",
        "ambient",
    ];
    // OCI's LinuxCapabilities has exactly these five sets; any other key is
    // capability data this decision would otherwise silently ignore.
    if let Some(unknown) = capabilities
        .keys()
        .find(|key| !SETS.contains(&key.as_str()))
    {
        return Err(format!(
            "native capability data holds an unknown set `{unknown}`"
        ));
    }
    if !capabilities.contains_key("bounding") {
        return Err("native Spec.process.capabilities has no bounding set".to_owned());
    }
    let mut absent = true;
    for set in SETS {
        let Some(value) = capabilities.get(set) else {
            continue;
        };
        let names = value
            .as_array()
            .ok_or_else(|| format!("native capability set `{set}` is not a list"))?;
        for name in names {
            let name = name
                .as_str()
                .filter(|name| LINUX_CAPABILITY_NAMES.contains(name))
                .ok_or_else(|| format!("native capability set `{set}` holds an unknown entry"))?;
            absent &= NERDCTL_DEFAULT_CAPS.contains(&name);
        }
    }
    // This path reads the compatibility record strictly: a missing or malformed
    // field is not an unprivileged, add-back-free builder.
    let unprivileged_without_add_back = nerdctl_compat_add_back_witness(record)?;
    Ok(absent && unprivileged_without_add_back)
}

/// The nerdctl compatibility record must state `Privileged` as a boolean and
/// `CapAdd` as a list. Only `Privileged: false` with an empty `CapAdd` is
/// `Ok(true)`. A privileged builder or an added capability is a well-formed
/// `Ok(false)`, and a missing or malformed field is an error.
fn nerdctl_compat_add_back_witness(record: Option<&serde_json::Value>) -> Result<bool, String> {
    let host = record
        .and_then(|record| record.get("HostConfig"))
        .and_then(|value| value.as_object())
        .ok_or_else(|| "compatibility record has no HostConfig".to_owned())?;
    let privileged = host
        .get("Privileged")
        .and_then(|value| value.as_bool())
        .ok_or_else(|| {
            "compatibility HostConfig.Privileged is missing or not a boolean".to_owned()
        })?;
    let cap_add = host
        .get("CapAdd")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "compatibility HostConfig.CapAdd is missing or not a list".to_owned())?;
    Ok(!privileged && cap_add.is_empty())
}

/// Native inspect of one full immutable ID. The compatibility record is only
/// the identity source; this read never follows a name.
fn inspect_native_spec_args(id: &str) -> [OsString; 3] {
    [
        OsString::from("inspect"),
        OsString::from("--mode=native"),
        OsString::from(id),
    ]
}

#[derive(Clone, Copy)]
enum WorkspaceBuilderState {
    NeverStarted,
    Stopped,
    Running,
    Unknown,
}

struct InspectedWorkspaceBuilder {
    state: WorkspaceBuilderState,
    binding: VerifiedComposeContainer,
    observed: ObservedWorkspaceArtifactBuilder,
    source_owned: bool,
    matching: bool,
}

/// Capture exactly once; the compatibility record supplies identity, mounts,
/// labels and state together. nerdctl's native capability witness is a separate
/// read of that same full ID, never a second compatibility observation.
fn inspect_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<InspectedWorkspaceBuilder, ContainerExecError> {
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    let remaining =
        super::process::remaining_until_deadline(deadline, "inspect workspace builder")?;
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &inspect_owned_binding_args(row),
        "inspect workspace builder",
        deadline,
        remaining,
    )?;
    let raw = serde_json::from_slice(&output.stdout).map_err(|error| {
        ContainerExecError::refused(
            "inspect workspace builder",
            format!("inspect output is not JSON: {error}"),
        )
    })?;
    let backend = crate::compose::resolve_compose_backend_for_repo(repo_root, policy);
    let mut inspected = decode_workspace_builder(&ownership, policy, row, &raw, backend)?;
    inspected.observed.remount_capability_dropped = remount_capability_witness(
        repo_root,
        policy,
        inspected.binding.id(),
        raw.as_array().and_then(|records| records.first()),
        deadline,
    )?;
    inspected.matching &= inspected.observed.remount_capability_dropped;
    Ok(inspected)
}

fn decode_workspace_builder(
    ownership: &OwnershipContext,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    raw: &serde_json::Value,
    backend: crate::compose::ComposeBackend,
) -> Result<InspectedWorkspaceBuilder, ContainerExecError> {
    let refuse = |reason| ContainerExecError::refused("inspect workspace builder", reason);
    if row.service.as_deref() != Some("builder") || row.oneoff {
        return Err(refuse(
            "workspace artifact builder service identity is unverified".to_owned(),
        ));
    }
    let never_started = row.status.lifecycle == super::parse::ContainerLifecycle::Created;
    if never_started && !matches!(backend, crate::compose::ComposeBackend::ColimaNerdctl) {
        return Err(refuse(
            "the never-started shape is proven only for the Colima nerdctl backend".to_owned(),
        ));
    }
    let identity = super::parse::inspected_identity_from_value(raw, never_started)
        .map_err(|error| refuse(format!("binding unavailable: {error}")))?;
    let record = raw.as_array().and_then(|records| records.first());
    let labels = record.and_then(|record| record.pointer("/Config/Labels"));
    let (checkout, oneoff_label_ok) = compose_binding_labels(labels);
    let scope = labels
        .and_then(|labels| labels.get(crate::runtime::scope::SCOPE_LABEL))
        .map(|value| value.as_str().unwrap_or_default());
    let explicit_checkout = labels
        .and_then(|labels| labels.get("com.effigy.checkout"))
        .and_then(|value| value.as_str());
    if !ownership.inspected_identity_matches(policy, row, &identity, oneoff_label_ok, false)
        || !ownership.owns(checkout, scope)
        || !ownership.owns(explicit_checkout, scope)
        || (never_started && identity.name != format!("{}-builder-1", policy.project_name))
    {
        return Err(refuse(format!("binding unavailable: full-ID/project/service/oneoff/running/checkout/scope ownership does not match owned project `{}` at `{}`", policy.project_name, ownership.canonical.display())));
    }
    if never_started {
        use std::os::unix::fs::MetadataExt;
        let root = std::fs::metadata(&ownership.canonical)
            .map_err(|error| refuse(format!("the checkout is not readable: {error}")))?;
        let object_identity = |name: &str| {
            let value: serde_json::Value =
                serde_json::from_str(labels?.get(name)?.as_str()?).ok()?;
            Some((
                value.get("device")?.as_u64()?,
                value.get("inode")?.as_u64()?,
            ))
        };
        if object_identity(CHECKOUT_GENERATION_LABEL) != Some((root.dev(), root.ino())) {
            return Err(refuse(
                "binding unavailable: checkout generation is stale or unverified".to_owned(),
            ));
        }
        if object_identity(CHECKOUT_FENCE_LABEL).map(|(device, _)| device) != Some(root.dev()) {
            return Err(refuse(
                "binding unavailable: checkout fence is unverified".to_owned(),
            ));
        }
    }
    let mounts = record
        .and_then(|record| record.get("Mounts"))
        .and_then(|value| value.as_array());
    let (source, read_only) = workspace_builder_source_mount(mounts).ok_or_else(|| {
        refuse("workspace builder binds no single `/workspace` source".to_owned())
    })?;
    let mounts_owned = builder_cache_volumes_are_owned(mounts, &policy.project_name);
    let no_ports = workspace_builder_has_no_published_ports(record);
    let working_dir_matches = builder_working_directory_is_owned(
        &ownership.canonical,
        ownership.staged_render.as_ref(),
        record.and_then(|record| {
            record.pointer("/Config/Labels/com.docker.compose.project.working_dir")
        }),
    );
    if !no_ports
        || !working_dir_matches
        || (never_started && (!mounts_owned || source != ownership.canonical))
    {
        return Err(refuse("workspace builder source, cargo cache volumes, published ports or Compose working directory are unverified".to_owned()));
    }
    let state = if never_started {
        WorkspaceBuilderState::NeverStarted
    } else if identity.running && super::parse::compose_status_is_running(&identity.status) {
        WorkspaceBuilderState::Running
    } else if !identity.running
        && super::parse::ContainerStatus::from(identity.status.clone()).lifecycle
            == super::parse::ContainerLifecycle::Exited
    {
        WorkspaceBuilderState::Stopped
    } else {
        WorkspaceBuilderState::Unknown
    };
    let expected = expected_workspace_source(&ownership.canonical, policy)?;
    let matching = read_only && source == expected && mounts_owned;
    let source_owned = source == ownership.canonical || source == expected;
    let binding = VerifiedComposeContainer {
        row: row.clone(),
        id: identity.id.clone(),
        checkout: ownership.canonical.clone(),
        scope: ownership
            .scope_token
            .as_deref()
            .map(|scope| scope.trim().to_owned()),
        profile: policy.profile.clone(),
        backend,
        workspace_artifact_builder: true,
    };
    Ok(InspectedWorkspaceBuilder {
        state,
        binding,
        observed: ObservedWorkspaceArtifactBuilder {
            id: identity.id,
            source,
            never_started,
            read_only,
            mounts_owned,
            remount_capability_dropped: false,
        },
        source_owned,
        matching,
    })
}

/// The one `/workspace` source the builder's mounts resolve to, with nothing
/// mounted below it. A name, label or volume never substitutes for it, so a
/// builder belonging to another checkout's source (or one bound to a VM
/// directory that does not exist on the host) is never adopted.
///
/// Identification does not require the bind to be writable: an owned read-write
/// leftover is observed so the caller can refuse or recover it. Execution
/// authority still requires `RW=false`.
fn workspace_builder_source_mount(
    mounts: Option<&Vec<serde_json::Value>>,
) -> Option<(PathBuf, bool)> {
    let mounts = mounts?;
    let workspace_mounts = mounts
        .iter()
        .filter(|mount| {
            mount.get("Destination").and_then(|value| value.as_str()) == Some("/workspace")
        })
        .collect::<Vec<_>>();
    let shadowed = mounts.iter().any(|mount| {
        mount
            .get("Destination")
            .and_then(|value| value.as_str())
            .is_some_and(|destination| destination.starts_with("/workspace/"))
    });
    if shadowed {
        return None;
    }
    let [mount] = workspace_mounts.as_slice() else {
        return None;
    };
    if mount.get("Type").and_then(|value| value.as_str()) != Some("bind") {
        return None;
    }
    let read_only = mount.get("RW").and_then(|value| value.as_bool()) == Some(false);
    let source = mount
        .get("Source")
        .and_then(|value| value.as_str())
        .and_then(|value| Path::new(value).canonicalize().ok())?;
    Some((source, read_only))
}

/// Where this environment's builder source must be: the canonical checkout, or
/// the checkout's owned staged source when the runtime profile cannot reach the
/// checkout's own host path (effigy#715). Both the render and this proof resolve
/// the same plan, so a render cannot bind a source the proof would not accept.
pub fn expected_workspace_source(
    canonical_checkout: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<PathBuf, ContainerExecError> {
    let stage = crate::artifact_source::policy_stage(policy).map_err(|error| {
        ContainerExecError::refused("resolve workspace builder source", error.to_string())
    })?;
    let Some(plan) = stage else {
        return Ok(canonical_checkout.to_path_buf());
    };
    plan.source.canonicalize().map_err(|error| {
        ContainerExecError::refused(
            "resolve workspace builder source",
            format!(
                "the owned staged source `{}` this checkout binds is not readable: {error}",
                plan.source.display()
            ),
        )
    })
}

/// The builder declares no route, so an inspected record with published
/// associations is not the private builder. A missing `NetworkSettings.Ports`
/// object is not proof of "no ports".
fn workspace_builder_has_no_published_ports(record: Option<&serde_json::Value>) -> bool {
    record
        .and_then(|record| record.pointer("/NetworkSettings/Ports"))
        .and_then(|value| value.as_object())
        .is_some_and(|ports| ports.is_empty())
}

fn builder_working_directory_is_owned(
    checkout: &Path,
    staged: Option<&crate::artifact_source::StagePlan>,
    label: Option<&serde_json::Value>,
) -> bool {
    let Some(label) = label else { return true };
    let Some(path) = label
        .as_str()
        .and_then(|value| Path::new(value).canonicalize().ok())
    else {
        return false;
    };
    if path == checkout {
        return true;
    }
    if staged.is_some_and(|plan| plan.owns_staged_render_directory(&path)) {
        return true;
    }
    if !path.is_dir() {
        return false;
    }
    let Ok(relative) = path.strip_prefix(checkout.join(".effigy/state/objects")) else {
        return false;
    };
    let Some(digest) = relative
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())
    else {
        return false;
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn verify_owned_running_compose_rows(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    deadline: Instant,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    verify_owned_running_compose_identities(repo_root, policy, rows, deadline)
        .map(|verified| verified.into_iter().map(|binding| binding.row).collect())
}

/// Whether all declared services, including the primary, are running. The
/// caller separately verifies ownership before using this to avoid Compose up.
pub fn compose_stack_is_running(
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
) -> Result<bool, ContainerExecError> {
    let mut declared = declared_service_names(policy, &policy.compose_files)?;
    declared.insert(policy.primary_service.clone());
    Ok(declared.iter().all(|service| {
        rows.iter().any(|row| {
            row.project_name.as_deref() == Some(policy.project_name.as_str())
                && row.service.as_deref() == Some(service.as_str())
                && !row.oneoff
                && row.status.is_running()
        })
    }))
}

pub fn verify_owned_running_compose_identities(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    deadline: Instant,
) -> Result<Vec<VerifiedComposeContainer>, ContainerExecError> {
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    let mut verified = Vec::new();
    for row in rows
        .iter()
        .filter(|row| row.project_name.as_deref() == Some(&policy.project_name))
    {
        if row.oneoff {
            continue;
        }
        let remaining =
            super::process::remaining_until_deadline(deadline, "inspect owned binding")?;
        let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
            repo_root,
            policy,
            &inspect_owned_binding_args(row),
            "inspect owned binding",
            deadline,
            remaining,
        )?;
        verified.push(ownership.verify_inspected(repo_root, policy, row, &output.stdout)?);
    }
    Ok(verified)
}

/// The full ID of one shared-service container the restart-policy apply may
/// update. A shared stack belongs to no checkout, so its container is proven by
/// its own inspected identity: full ID, shared project, declared service,
/// running state and the render label equal to the render `up` just published.
pub fn verify_shared_render_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
    service: &str,
    container_name: &str,
    render_digest: &str,
    deadline: Instant,
) -> Result<String, ContainerExecError> {
    let remaining = super::process::remaining_until_deadline(deadline, "inspect shared container")?;
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &[OsString::from("inspect"), OsString::from(container_name)],
        "inspect shared container",
        deadline,
        remaining,
    )?;
    shared_render_identity(
        &output.stdout,
        project_name,
        service,
        container_name,
        render_digest,
    )
}

/// Prove one inspect record is the shared container `container_name` of
/// `project_name` and `service`, running and rendered from `render_digest`,
/// and return its full ID. Anything else is refused with its reason.
pub fn shared_render_identity(
    stdout: &[u8],
    project_name: &str,
    service: &str,
    container_name: &str,
    render_digest: &str,
) -> Result<String, ContainerExecError> {
    let refuse = |reason: String| ContainerExecError::Failure {
        command: "inspect shared container".to_owned(),
        code: None,
        stdout: String::new(),
        stderr: format!("binding unavailable: {reason}"),
    };
    let identity =
        super::parse::parse_inspected_container_identity(&String::from_utf8_lossy(stdout))
            .map_err(|error| {
                refuse(format!(
                    "no canonical full identity for `{container_name}`: {error}"
                ))
            })?;
    let raw: serde_json::Value = serde_json::from_slice(stdout)
        .map_err(|_| refuse(format!("inspect of `{container_name}` is not JSON")))?;
    let render = raw
        .as_array()
        .and_then(|records| records.first())
        .and_then(|record| record.get("Config"))
        .and_then(|config| config.get("Labels"))
        .and_then(|labels| labels.get(crate::runtime::scope::SHARED_RENDER_LABEL))
        .and_then(|value| value.as_str());
    if identity.name != container_name {
        return Err(refuse(format!(
            "`{container_name}` now inspects as `{}`",
            identity.name
        )));
    }
    if identity.project.as_deref() != Some(project_name) {
        return Err(refuse(format!(
            "`{container_name}` belongs to project `{}`, not shared project `{project_name}`",
            identity.project.as_deref().unwrap_or("(none)")
        )));
    }
    if identity.service.as_deref() != Some(service) || identity.oneoff {
        return Err(refuse(format!(
            "`{container_name}` is not the declared shared service `{service}`"
        )));
    }
    if !(identity.running && super::parse::compose_status_is_running(&identity.status)) {
        return Err(refuse(format!("`{container_name}` is not running")));
    }
    match render {
        Some(rendered) if rendered == render_digest => Ok(identity.id),
        Some(rendered) => Err(refuse(format!(
            "`{container_name}` was rendered from `{rendered}`, not the current render `{render_digest}`"
        ))),
        None => Err(refuse(format!(
            "`{container_name}` carries no shared render label"
        ))),
    }
}

const OWNED_SERVICE_LISTING_FORMAT: &str = "{{.ID}}\t{{.Status}}\t{{.Label \"com.docker.compose.oneoff\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.effigy.checkout\"}}\t{{.Label \"com.effigy.scope\"}}";

/// Runtime `ps` arguments listing the containers of one project service with
/// the fields [`OwnershipContext::verify_listed_service`] checks.
pub fn owned_service_listing_args(
    policy: &EffectiveContainerPolicy,
    service: &str,
) -> Vec<OsString> {
    [
        "ps".to_owned(),
        "--no-trunc".to_owned(),
        "--filter".to_owned(),
        format!("label=com.docker.compose.project={}", policy.project_name),
        "--filter".to_owned(),
        format!("label=com.docker.compose.service={service}"),
        "--format".to_owned(),
        OWNED_SERVICE_LISTING_FORMAT.to_owned(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

/// Runtime arguments that inspect one listed container for ownership checks.
fn inspect_owned_binding_args(row: &RunningComposeContainer) -> [OsString; 2] {
    [
        OsString::from("inspect"),
        OsString::from(&row.container_name),
    ]
}

/// The service names the given Compose files declare.
pub(super) fn declared_service_names(
    policy: &EffectiveContainerPolicy,
    compose_files: &[PathBuf],
) -> Result<BTreeSet<String>, ContainerExecError> {
    let mut names = BTreeSet::new();
    for compose_file in compose_files {
        let content =
            policy
                .read_compose(compose_file)
                .map_err(|error| ContainerExecError::Failure {
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

/// The selected checkout's identity, resolved once and compared against each
/// inspected container. Callers that must account every runtime capture (the
/// bounded command path) run `inspect` themselves and pass its output to
/// [`OwnershipContext::verify_inspected`].
pub struct OwnershipContext {
    declared: BTreeSet<String>,
    canonical: std::path::PathBuf,
    scope_token: Option<String>,
    /// The checkout's own staged Compose render, when this environment stages
    /// one under the runtime profile's mount (effigy#715). A Compose container
    /// started from it reports that directory as its project working directory.
    staged_render: Option<crate::artifact_source::StagePlan>,
}

impl OwnershipContext {
    pub fn resolve(
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
    ) -> Result<Self, ContainerExecError> {
        let declared = declared_service_names(policy, &policy.compose_files)?;
        // Ownership is the checkout's, not a nested catalog directory's: the
        // generated labels name the enclosing checkout (contract 052 §7.1).
        let canonical = effigy_core::worktree_scope::checkout_root(repo_root)
            .canonicalize()
            .map_err(|error| ContainerExecError::Launch {
                command: "verify selected checkout".to_owned(),
                error,
            })?;
        let scope_token = effigy_core::worktree_scope::resolve(repo_root)
            .map_err(|error| ContainerExecError::Launch {
                command: "verify selected scope".to_owned(),
                error,
            })?
            .map(|scope| {
                std::fs::read_to_string(
                    scope
                        .token_dir
                        .join(effigy_core::worktree_scope::SCOPE_FILE),
                )
            })
            .transpose()
            .map_err(|error| ContainerExecError::Launch {
                command: "verify selected scope token".to_owned(),
                error,
            })?;
        let staged_render = crate::artifact_source::policy_stage(policy).map_err(|error| {
            ContainerExecError::refused("resolve owned Compose render", error.to_string())
        })?;
        Ok(Self {
            declared,
            canonical,
            scope_token,
            staged_render,
        })
    }

    /// The identity fields every admitted container must satisfy regardless of
    /// checkout or scope labels: the listed container's name, the selected
    /// project, a declared service, not one-off, and - only when the caller
    /// requires an active workload - running.
    fn inspected_identity_matches(
        &self,
        policy: &EffectiveContainerPolicy,
        row: &RunningComposeContainer,
        identity: &super::parse::InspectedContainerIdentity,
        oneoff_label_ok: bool,
        require_running: bool,
    ) -> bool {
        identity.name == row.container_name
            && identity.project.as_deref() == Some(policy.project_name.as_str())
            && identity.service == row.service
            && identity
                .service
                .as_ref()
                .is_some_and(|service| self.declared.contains(service))
            && (!require_running
                || (identity.running && super::parse::compose_status_is_running(&identity.status)))
            && !identity.oneoff
            && oneoff_label_ok
    }

    /// Whether a container's checkout label and scope label name this
    /// checkout and scope.
    fn owns(&self, checkout: Option<&str>, scope: Option<&str>) -> bool {
        let scope_matches = match (self.scope_token.as_deref(), scope) {
            (None, None) => true,
            (Some(token), Some(value)) => !token.trim().is_empty() && value == token.trim(),
            _ => false,
        };
        if !scope_matches {
            return false;
        }
        let Some(path) = checkout.and_then(|path| Path::new(path).canonicalize().ok()) else {
            return false;
        };
        path == self.canonical
            || self
                .staged_render
                .as_ref()
                .is_some_and(|plan| plan.owns_staged_render_directory(&path))
    }

    /// Verify every full-ID project row before teardown, including stopped
    /// containers and orphans. Unlike exec/publication this needs no running
    /// state, inspect snapshot, declared service, or port associations.
    pub(super) fn verify_shutdown_listing(
        &self,
        policy: &EffectiveContainerPolicy,
        stdout: &[u8],
    ) -> Result<(), ContainerExecError> {
        for line in String::from_utf8_lossy(stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            let fields = line.split('\t').collect::<Vec<_>>();
            if let [id, project, working_dir, checkout, scope] = fields[..] {
                fn present(value: &str) -> Option<&str> {
                    (!value.is_empty()).then_some(value)
                }
                if super::parse::parse_full_container_id(id).is_some()
                    && project == policy.project_name
                    && self.owns(
                        checkout_label(present(working_dir), present(checkout), present(scope)),
                        present(scope),
                    )
                {
                    continue;
                }
            }
            return Err(ContainerExecError::Failure {
                command: "verify managed shutdown ownership".to_owned(), code: None,
                stdout: String::new(), stderr: "project container has unknown or foreign full-ID/checkout/scope ownership; no teardown ran".to_owned(),
            });
        }
        Ok(())
    }

    /// Prove that an [`owned_service_listing_args`] listing holds exactly one
    /// running, non-oneoff `service` container of this checkout and scope, and
    /// return its full ID. Callers that must keep runtime captures small (the
    /// bounded command path) use this instead of a full `inspect`.
    pub fn verify_listed_service(
        &self,
        service: &str,
        listing_stdout: &[u8],
    ) -> Result<String, ContainerExecError> {
        let refuse = |reason: &str| {
            ContainerExecError::refused(
                "list owned service",
                format!("binding unavailable: {reason}"),
            )
        };
        if !self.declared.contains(service) {
            return Err(refuse(
                "service is not declared by the selected Compose files",
            ));
        }
        let listing = String::from_utf8_lossy(listing_stdout);
        let mut rows = listing.lines().filter(|line| !line.trim().is_empty());
        let row = rows
            .next()
            .ok_or_else(|| refuse("no running container for the service"))?;
        if rows.next().is_some() {
            return Err(refuse("more than one container for the service"));
        }
        let fields = row.split('\t').map(str::trim).collect::<Vec<_>>();
        let [id, status, oneoff, working_dir, checkout, scope] = fields[..] else {
            return Err(refuse("service listing is malformed"));
        };
        // Listings print an empty string for an absent label.
        fn present(value: &str) -> Option<&str> {
            Some(value).filter(|value| !value.is_empty())
        }
        let oneoff = matches!(oneoff.to_ascii_lowercase().as_str(), "true" | "1" | "yes");
        if id.len() != 64
            || !id.bytes().all(|byte| byte.is_ascii_hexdigit())
            || !super::parse::compose_status_is_running(status)
            || oneoff
            || !self.owns(
                checkout_label(present(working_dir), present(checkout), present(scope)),
                present(scope),
            )
        {
            return Err(refuse(
                "full-ID/oneoff/running/checkout/scope ownership changed or is unknown",
            ));
        }
        Ok(id.to_owned())
    }

    /// Prove that `inspect_stdout` describes `row` as a running, non-oneoff
    /// service of this checkout and scope.
    pub fn verify_inspected(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        row: &RunningComposeContainer,
        inspect_stdout: &[u8],
    ) -> Result<VerifiedComposeContainer, ContainerExecError> {
        let raw: serde_json::Value = serde_json::from_slice(inspect_stdout).map_err(|error| {
            ContainerExecError::refused(
                "inspect owned binding",
                format!("binding unavailable: inspect JSON is invalid: {error}"),
            )
        })?;
        let identity =
            super::parse::inspected_identity_from_value(&raw, false).map_err(|error| {
                // The parser's own cause is the diagnosis: a missing canonical
                // identity is one reason among malformed JSON, a wrong record
                // count, a short id, a missing name and a missing State.Status.
                ContainerExecError::refused(
                    "inspect owned binding",
                    format!(
                        "binding unavailable: canonical container identity is unverified: {error}"
                    ),
                )
            })?;
        let record = raw.as_array().and_then(|records| records.first());
        let labels = record
            .and_then(|record| record.get("Config"))
            .and_then(|config| config.get("Labels"));
        let (working_dir, oneoff_label_ok) = compose_binding_labels(labels);
        let inspected_scope = labels
            .and_then(|labels| labels.get(crate::runtime::scope::SCOPE_LABEL))
            .map(|value| value.as_str().unwrap_or_default());
        let owned = self.inspected_identity_matches(policy, row, &identity, oneoff_label_ok, true)
            && self.owns(working_dir, inspected_scope);
        if !owned {
            return Err(ContainerExecError::refused(
                "inspect owned binding",
                format!(
                    "binding unavailable: full-ID/project/service/oneoff/running/checkout/scope ownership does not match owned project `{}` at `{}`",
                    policy.project_name,
                    self.canonical.display(),
                ),
            ));
        }
        let Some(ports) = record
            .and_then(|record| record.get("NetworkSettings"))
            .and_then(|network| network.get("Ports"))
            .and_then(|ports| ports.as_object())
        else {
            return Err(ContainerExecError::refused(
                "inspect owned binding",
                "binding unavailable: fresh published-port associations are missing",
            ));
        };
        let mut current = row.clone();
        current.ports.clear();
        for (container_port, bindings) in ports {
            for binding in bindings.as_array().into_iter().flatten() {
                let host_ip = binding.get("HostIp").and_then(|value| value.as_str());
                let host_port = binding.get("HostPort").and_then(|value| value.as_str());
                if let (Some(ip), Some(port)) = (host_ip, host_port) {
                    if matches!(ip, "127.0.0.1" | "0.0.0.0")
                        && port.parse::<u16>().is_ok_and(|port| port != 0)
                    {
                        current
                            .ports
                            .push(format!("{ip}:{port}->{container_port}").into());
                    }
                }
            }
        }
        Ok(VerifiedComposeContainer {
            workspace_artifact_builder: false,
            row: current,
            id: identity.id,
            checkout: self.canonical.clone(),
            scope: self
                .scope_token
                .as_deref()
                .map(|scope| scope.trim().to_owned()),
            profile: policy.profile.clone(),
            backend: crate::compose::resolve_compose_backend_for_repo(repo_root, policy),
        })
    }
}

pub fn list_compose_containers_for_project_including_stopped(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    project_name: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = retry_inventory_once(|| {
        capture_runtime_command(
            repo_root,
            policy,
            &project_ps_args(project_name, true),
            "runtime ps --all",
            Some(COMPOSE_PS_TIMEOUT),
            None,
        )
    })?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(project_name))
            .collect(),
    )
}

/// Retry one read-only runtime inventory once when Effigy's own bound expired.
/// A machine under load can miss a single 30s observation; a parse error, a
/// refused runtime or a second timeout is returned unchanged so admission still
/// fails closed and the message names the runtime observation, not the task.
fn retry_inventory_once(
    mut run: impl FnMut() -> Result<Output, ContainerExecError>,
) -> Result<Output, ContainerExecError> {
    match run() {
        Err(error) if super::process::error_is_timeout(&error) => {
            std::thread::sleep(INVENTORY_RETRY_BACKOFF);
            run().map_err(|second| match second {
                ContainerExecError::Failure {
                    command,
                    code,
                    stdout,
                    stderr,
                } => ContainerExecError::Failure {
                    command,
                    code,
                    stdout,
                    stderr: format!(
                        "{stderr}\n[effigy] runtime inventory admission timed out twice; the runtime did not answer an observation, so the task did not run. Check `effigy container status` and retry, or run the task's host route with `cargo` by hand where the repository defines one. First timeout: {error}"
                    ),
                },
                other => other,
            })
        }
        result => result,
    }
}

pub fn list_compose_containers_for_policy_including_stopped(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    list_compose_containers_for_project_including_stopped(
        repo_root,
        policy,
        policy.project_name.as_str(),
    )
}

pub fn list_compose_containers_for_policy_including_stopped_with_timeout(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    timeout: Duration,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &project_ps_args(policy.project_name.as_str(), true),
        "runtime ps --all",
        timeout,
    )?;
    Ok(
        parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))?
            .into_iter()
            .filter(|row| row.project_name.as_deref() == Some(policy.project_name.as_str()))
            .collect(),
    )
}

/// `runtime inspect` for the named containers of one policy's backend.
///
/// A readiness wait reads every service's own state and health check from
/// here; the bounded deadline is the wait's, so a slow runtime cannot extend
/// it (effigy#313).
pub fn inspect_containers_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    names: &[String],
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let mut args = vec![OsString::from("inspect")];
    args.extend(names.iter().map(OsString::from));
    capture_runtime_command(
        repo_root,
        policy,
        &args,
        "runtime inspect",
        Some(COMPOSE_PS_TIMEOUT),
        deadline,
    )
}

/// `runtime image inspect` for one exact image. The caller reads the platform
/// the backend reports for the image itself; a container's own report, a host
/// default or a guess never substitutes for it.
pub fn inspect_image_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    image: &str,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let args = vec![
        OsString::from("image"),
        OsString::from("inspect"),
        OsString::from(image),
    ];
    capture_runtime_command(
        repo_root,
        policy,
        &args,
        "runtime image inspect",
        Some(COMPOSE_PS_TIMEOUT),
        deadline,
    )
}

/// The one line that says why a container stopped: the last line it logged
/// that names a cause, else the last line it logged. Read on demand for one
/// failure diagnostic; only exited or unhealthy services are read this way, so
/// a healthy stack pays nothing.
pub fn container_last_log_line_for_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container: &str,
    tail: u32,
) -> Result<String, ContainerExecError> {
    let args = vec![
        OsString::from("logs"),
        OsString::from("--tail"),
        OsString::from(tail.to_string()),
        OsString::from(container),
    ];
    let output = run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        &args,
        "runtime logs",
        COMPOSE_PS_TIMEOUT,
    )?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok(effigy_ui::human::last_output_line(&combined).unwrap_or_default())
}

/// One runtime capture. A failure is reported as it stands: Effigy never
/// stops or restarts the shared Colima profile to "repair" it (effigy#371).
///
/// Every capture is bounded: a read that names no deadline and no timeout is
/// still the standard runtime read, so a runtime that stops answering reports
/// that read as unavailable instead of holding the command open (effigy#411).
fn capture_runtime_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
    label: &str,
    timeout: Option<Duration>,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    if let Some(deadline) = deadline {
        let remaining = super::process::remaining_until_deadline(deadline, label)?;
        return run_runtime_command_capture_for_policy_with_absolute_deadline(
            repo_root, policy, args, label, deadline, remaining,
        );
    }
    run_runtime_command_capture_for_policy_with_timeout(
        repo_root,
        policy,
        args,
        label,
        timeout.unwrap_or(COMPOSE_PS_TIMEOUT),
    )
}

/// Runs one invocation an environment backend adapter resolved for its exact
/// selection. Detached Compose activation never inherits a capture pipe.
pub fn run_backend_invocation(
    workdir: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    if args.iter().any(|arg| arg == "up") && args.iter().any(|arg| arg == "-d" || arg == "--detach")
    {
        return run_detached_command_with_env(workdir, &program, args, label, env);
    }
    run_command_capture_os_with_env(workdir, &program, args, label, env)
}

/// [`run_backend_invocation`] bounded by the caller's absolute deadline.
/// Detached Compose keeps regular-file output custody and retained logs;
/// observations use bounded capture. Both share the activation's deadline.
pub fn run_backend_invocation_until(
    workdir: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    if args.iter().any(|arg| arg == "up") && args.iter().any(|arg| arg == "-d" || arg == "--detach")
    {
        return super::process::run_detached_command_with_env_until(
            workdir, &program, args, label, env, deadline,
        );
    }
    super::process::run_command_capture_os_with_env_until(
        workdir, &program, args, label, env, deadline,
    )
}

/// Refuse private execution until the reviewed same-Env capability exists.
/// Every input reaches the same refusal before interpolation or process work.
pub fn run_backend_invocation_private_until(
    workdir: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    deadline: Instant,
) -> Result<(), ContainerExecError> {
    super::process::run_private_detached_command(
        workdir,
        &program.to_string_lossy(),
        args,
        label,
        env,
        deadline,
    )
}

pub fn run_compose_invocation_capture(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_compose_invocation_capture_with_env(repo_root, program, args, label, &[])
}

pub fn run_compose_invocation_capture_with_env(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    if args.iter().any(|arg| arg == "up") && args.iter().any(|arg| arg == "-d" || arg == "--detach")
    {
        return run_detached_command_with_env(repo_root, &program, args, label, env);
    }
    run_command_capture_os_with_env(repo_root, &program, args, label, env)
}

/// Run one Compose invocation whose progress a person is watching.
///
/// The command's output still lands in the checkout's retained backend
/// record, exactly as a detached invocation does; every complete line is
/// reported to `on_line` as it is written, so the caller can name what the
/// runtime is doing while it does it (effigy#313). A stop request ends the
/// invocation's whole process group and returns its status.
#[allow(
    clippy::too_many_arguments,
    reason = "one invocation needs its policy, argv, env, stop flag and line sink"
)]
pub fn run_compose_invocation_streaming_with_env(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    program: &OsStr,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
    on_line: &mut dyn FnMut(&str),
) -> Result<Output, ContainerExecError> {
    let program = program.to_string_lossy();
    match run_detached_command_streaming_with_env(
        repo_root,
        &program,
        args,
        label,
        env,
        stop_requested,
        on_line,
    ) {
        Ok(output) => Ok(output),
        // nerdctl keeps its own name store. A container it left behind makes
        // replacing a service that really changed fail on that name; clear the
        // stale one and retry once.
        Err(error) => {
            if let ContainerExecError::Failure { stdout, stderr, .. } = &error {
                if take_stale_owned_container(repo_root, policy, stdout, stderr)? {
                    return run_detached_command_streaming_with_env(
                        repo_root,
                        &program,
                        args,
                        label,
                        env,
                        stop_requested,
                        on_line,
                    );
                }
            }
            Err(error)
        }
    }
}

/// Remove the container nerdctl's name store still holds when it blocked a
/// create, and report that the caller may retry once.
///
/// A container that carries another project's `com.docker.compose.project`
/// label is never touched: its name conflict is reported as it stands.
fn take_stale_owned_container(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    stdout: &str,
    stderr: &str,
) -> Result<bool, ContainerExecError> {
    let Some(claimed) = name_store_conflict(&format!("{stdout}\n{stderr}")) else {
        return Ok(false);
    };
    let Ok(observed) =
        inspect_containers_for_policy(repo_root, policy, std::slice::from_ref(&claimed), None)
    else {
        return Ok(false);
    };
    let project = serde_json::from_slice::<serde_json::Value>(&observed.stdout)
        .ok()
        .and_then(|value| {
            let labels = value.get(0)?.get("Config")?.get("Labels")?;
            labels
                .get(crate::COMPOSE_PROJECT_LABEL)?
                .as_str()
                .map(str::to_owned)
        });
    if project.as_deref() != Some(policy.project_name.as_str()) {
        return Ok(false);
    }
    // A running container's `rm -f` is a kill the restart manager may undo for
    // `restart: unless-stopped`; stopping first records a user stop, so the
    // removal holds (effigy#376). A stop that finds nothing running is fine.
    let _ = run_docker_capture(
        repo_root,
        policy,
        &[OsString::from("stop"), OsString::from(&claimed)],
        "runtime stop",
    );
    let removed = run_docker_capture(
        repo_root,
        policy,
        &[
            OsString::from("rm"),
            OsString::from("-f"),
            OsString::from(&claimed),
        ],
        "runtime rm",
    )?;
    Ok(removed.status.success())
}

/// The identifier nerdctl names when a create lost to its own name store:
/// `name "<container>" is already used by ID <id>`.
fn name_store_conflict(message: &str) -> Option<String> {
    let marker = "is already used by id";
    let start = message.to_ascii_lowercase().find(marker)? + marker.len();
    message[start..]
        .split(|character: char| !character.is_ascii_hexdigit())
        .find(|candidate| candidate.len() >= 12)
        .map(str::to_owned)
}

pub fn list_running_compose_containers() -> Result<Vec<RunningComposeContainer>, ContainerExecError>
{
    Ok(discover_running_compose_containers()
        .into_complete_rows()?
        .into_iter()
        .map(|row| row.row)
        .collect())
}

pub fn list_running_compose_containers_for_profile(
    profile: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

pub fn list_running_compose_containers_profiled(
) -> Result<Vec<RunningComposeContainerProfiled>, ContainerExecError> {
    discover_running_compose_containers().into_complete_rows()
}

pub fn discover_running_compose_containers() -> RunningComposeContainerInventory {
    collect_running_compose_container_inventory(
        docker_runtime_participation(),
        list_running_compose_containers_for_docker,
        colima_runtime_participation(),
        // A running-container inventory only needs the running profiles; a
        // stopped profile has no running container to report.
        || running_colima_profiles(Path::new(".")).map(|running| (running, Vec::new())),
        list_running_compose_containers_for_profile,
    )
}

/// Collect the running Compose inventory under an explicit participation
/// verdict per runtime. A proven-inactive runtime is never probed, so it
/// cannot poison an otherwise complete inventory; every participating runtime
/// keeps its failure actionable.
fn collect_running_compose_container_inventory(
    docker_participation: RuntimeParticipation,
    docker_rows: impl FnOnce() -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
    colima_participation: RuntimeParticipation,
    colima_profiles: impl FnOnce() -> Result<(Vec<String>, Vec<String>), ContainerExecError>,
    mut profile_rows: impl FnMut(&str) -> Result<Vec<RunningComposeContainer>, ContainerExecError>,
) -> RunningComposeContainerInventory {
    let mut inventory = RunningComposeContainerInventory::default();
    if docker_participation == RuntimeParticipation::Participating {
        match docker_rows() {
            Ok(rows) => {
                inventory
                    .rows
                    .extend(rows.into_iter().map(|row| RunningComposeContainerProfiled {
                        profile: DOCKER_GLOBAL_RUNTIME_LABEL.to_owned(),
                        row,
                    }))
            }
            Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                backend: "docker".to_owned(),
                profile: "default".to_owned(),
                error: error.to_string(),
                stopped_profile: false,
            }),
        }
    }
    if colima_participation == RuntimeParticipation::Participating {
        match colima_profiles() {
            Ok((running, stopped)) => {
                for profile in stopped {
                    inventory.failures.push(RuntimeInventoryFailure {
                        backend: "colima".to_owned(),
                        error: "stopped colima profile cannot be inspected for created or stopped containers without starting it".to_owned(),
                        profile,
                        stopped_profile: true,
                    });
                }
                for profile in running {
                    match profile_rows(&profile) {
                        Ok(rows) => inventory.rows.extend(rows.into_iter().map(|row| {
                            RunningComposeContainerProfiled {
                                profile: profile.clone(),
                                row,
                            }
                        })),
                        Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                            backend: "colima".to_owned(),
                            profile,
                            error: error.to_string(),
                            stopped_profile: false,
                        }),
                    }
                }
            }
            Err(error) => inventory.failures.push(RuntimeInventoryFailure {
                backend: "colima".to_owned(),
                profile: "profile-list".to_owned(),
                error: error.to_string(),
                stopped_profile: false,
            }),
        }
    }
    inventory
}

/// Inventory every Compose container in every state (running, created,
/// exited, paused) across participating runtimes.
///
/// A stopped Colima profile is recorded as its own stopped-profile failure
/// instead of failing the whole inventory: its created or stopped containers
/// cannot be seen without starting the VM, but it publishes no host port
/// while stopped, so only owners recorded on that profile are uncertain
/// (effigy#294). Every other failure still makes the inventory incomplete.
pub fn discover_compose_containers_all_states() -> RunningComposeContainerInventory {
    collect_running_compose_container_inventory(
        docker_runtime_participation(),
        list_all_compose_containers_for_docker,
        colima_runtime_participation(),
        super::colima_runtime::colima_profiles_for_all_states_inventory,
        list_all_compose_containers_for_profile,
    )
}

fn list_all_compose_containers_for_docker(
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend(
        Path::new("."),
        BackendId::docker_compose(),
        &[
            OsString::from("ps"),
            OsString::from("--all"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps --all",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

fn list_all_compose_containers_for_profile(
    profile: &str,
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[
            OsString::from("ps"),
            OsString::from("--all"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps --all",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

pub fn infer_host_working_dir_for_container(
    profile: &str,
    container_name: &str,
) -> Result<Option<String>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend_profile(
        Path::new("."),
        BackendId::colima_nerdctl(),
        profile,
        &[OsString::from("inspect"), OsString::from(container_name)],
        "runtime inspect",
    )?;

    infer_host_working_dir_from_inspect(&String::from_utf8_lossy(&output.stdout)).map_err(|error| {
        ContainerExecError::Failure {
            command: "docker inspect".to_owned(),
            code: None,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: error,
        }
    })
}

pub fn capture_running_container_stats(container_names: &[String]) -> RunningContainerStatsCapture {
    match detect_container_backend() {
        Ok(backend) if backend == BackendId::docker_compose() => {
            capture_running_container_stats_for_backend(
                BackendId::docker_compose(),
                container_names,
            )
        }
        _ => capture_running_container_stats_for_profile(
            default_runtime_profile().as_str(),
            container_names,
        ),
    }
}

pub fn capture_running_container_stats_for_profile(
    profile: &str,
    container_names: &[String],
) -> RunningContainerStatsCapture {
    if container_names.is_empty() {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: None,
        };
    }

    let mut command = vec!["stats", "--no-stream", "--format", DOCKER_STATS_FORMAT];
    let names = container_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    command.extend(names.iter().copied());

    let output = if profile == DOCKER_GLOBAL_RUNTIME_LABEL {
        run_runtime_command_capture_allow_failure_for_backend(
            Path::new("."),
            BackendId::docker_compose(),
            &command.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    } else {
        run_runtime_command_capture_allow_failure_for_backend_profile(
            Path::new("."),
            BackendId::colima_nerdctl(),
            profile,
            &command.iter().map(OsString::from).collect::<Vec<_>>(),
        )
    };

    let Ok(output) = output else {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some("failed to launch runtime stats collection".to_owned()),
        };
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "runtime stats command failed ({})",
                effigy_ui::human::exit_code(output.status.code())
            )
        };
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(format!("resource stats unavailable: {detail}")),
        };
    }

    match parse_running_container_stats(&String::from_utf8_lossy(&output.stdout)) {
        Ok(stats) => {
            let stats_names = stats
                .iter()
                .map(|sample| sample.container_name.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let missing = container_names
                .iter()
                .filter(|name| !stats_names.contains(name.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let warning = if missing.is_empty() {
                None
            } else {
                Some(format!(
                    "runtime stats were unavailable for: {}",
                    missing.join(", ")
                ))
            };
            RunningContainerStatsCapture { stats, warning }
        }
        Err(error) => RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(format!("resource stats unavailable: {error}")),
        },
    }
}

fn list_running_compose_containers_for_docker(
) -> Result<Vec<RunningComposeContainer>, ContainerExecError> {
    let output = run_runtime_command_capture_for_backend(
        Path::new("."),
        BackendId::docker_compose(),
        &[
            OsString::from("ps"),
            OsString::from("--format"),
            OsString::from(DOCKER_PS_FORMAT),
        ],
        "runtime ps",
    )?;

    parse_running_compose_containers(&String::from_utf8_lossy(&output.stdout))
}

fn capture_running_container_stats_for_backend(
    backend: BackendId,
    container_names: &[String],
) -> RunningContainerStatsCapture {
    if container_names.is_empty() {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: None,
        };
    }

    let mut command = vec!["stats", "--no-stream", "--format", DOCKER_STATS_FORMAT];
    let names = container_names
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    command.extend(names.iter().copied());

    let output = run_runtime_command_capture_allow_failure_for_backend(
        Path::new("."),
        backend,
        &command.iter().map(OsString::from).collect::<Vec<_>>(),
    );

    let Ok(output) = output else {
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some("failed to launch runtime stats collection".to_owned()),
        };
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!(
                "runtime stats command failed ({})",
                effigy_ui::human::exit_code(output.status.code())
            )
        };
        return RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(detail),
        };
    }

    match parse_running_container_stats(&String::from_utf8_lossy(&output.stdout)) {
        Ok(stats) => RunningContainerStatsCapture {
            stats,
            warning: None,
        },
        Err(error) => RunningContainerStatsCapture {
            stats: Vec::new(),
            warning: Some(error.to_string()),
        },
    }
}

fn run_runtime_command_capture_for_backend(
    repo_root: &Path,
    backend: BackendId,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(
            &detection,
            default_runtime_profile().as_str(),
            "docker",
            docker_args,
        )
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_for_backend_profile(
    repo_root: &Path,
    backend: BackendId,
    profile: &str,
    docker_args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, profile, "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    run_command_capture_os(repo_root, &program, &args, label)
}

fn run_runtime_command_capture_allow_failure_for_backend(
    repo_root: &Path,
    backend: BackendId,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(
            &detection,
            default_runtime_profile().as_str(),
            "docker",
            docker_args,
        )
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = crate::compose::resolve_host_cli_program(&program);
    std::process::Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", super::process::format_args(&args)),
            error,
        })
}

fn run_runtime_command_capture_allow_failure_for_backend_profile(
    repo_root: &Path,
    backend: BackendId,
    profile: &str,
    docker_args: &[OsString],
) -> Result<Output, ContainerExecError> {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    detection.backend_override = Some(backend);
    let (program, args) = ContainerManager::defaults()
        .runtime_process_invocation(&detection, profile, "docker", docker_args)
        .map_err(container_manager_error)?;
    let program = program.to_string_lossy().into_owned();
    let resolved_program = crate::compose::resolve_host_cli_program(&program);
    std::process::Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args.iter())
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", super::process::format_args(&args)),
            error,
        })
}

#[cfg(test)]
mod inventory_tests {
    use super::*;
    use std::cell::Cell;

    fn timeout() -> ContainerExecError {
        ContainerExecError::Failure {
            command: "runtime ps --all".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "[effigy] command timed out after 30s".to_owned(),
        }
    }

    fn output() -> Output {
        std::process::Command::new("true")
            .output()
            .expect("fixture command")
    }

    #[test]
    fn a_single_inventory_timeout_is_retried_once() {
        let calls = Cell::new(0);
        let result = retry_inventory_once(|| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(timeout())
            } else {
                Ok(output())
            }
        })
        .expect("the second observation answers");
        assert!(result.status.success());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn a_second_inventory_timeout_names_runtime_admission() {
        let calls = Cell::new(0);
        let error = retry_inventory_once(|| {
            calls.set(calls.get() + 1);
            Err(timeout())
        })
        .expect_err("two timeouts refuse");
        assert_eq!(calls.get(), 2);
        let text = error.to_string();
        assert!(text.contains("timed out twice"), "{text}");
        assert!(text.contains("the task did not run"), "{text}");
        assert!(text.contains("by hand"), "{text}");
    }

    #[test]
    fn a_non_timeout_inventory_failure_is_never_retried() {
        let calls = Cell::new(0);
        let error = retry_inventory_once(|| {
            calls.set(calls.get() + 1);
            Err(ContainerExecError::Failure {
                command: "runtime ps --all".to_owned(),
                code: Some(1),
                stdout: String::new(),
                stderr: "Cannot connect to the Docker daemon".to_owned(),
            })
        })
        .expect_err("a refused runtime stays refused");
        assert_eq!(calls.get(), 1);
        assert!(!error.to_string().contains("timed out twice"));
    }

    fn row(project_name: &str, working_dir: &str) -> RunningComposeContainer {
        RunningComposeContainer {
            container_name: format!("{project_name}-1"),
            status: "Up 10 seconds".into(),
            ports: Vec::new(),
            project_name: Some(project_name.to_owned()),
            working_dir: Some(working_dir.to_owned()),
            service: Some("app".to_owned()),
            oneoff: false,
        }
    }

    fn failure(command: &str, stderr: &str) -> ContainerExecError {
        ContainerExecError::Failure {
            command: command.to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn complete_empty_inventory_is_distinct_from_discovery_failure() {
        let docker_probes = Cell::new(0);
        let colima_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Ok(Vec::new())
            },
            RuntimeParticipation::Participating,
            || {
                colima_probes.set(colima_probes.get() + 1);
                Ok((Vec::new(), Vec::new()))
            },
            |_| Ok(Vec::new()),
        );

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
        assert_eq!(docker_probes.get(), 1);
        assert_eq!(colima_probes.get(), 1);

        // The Colima-only shape: Docker is proven inactive (probing panics),
        // Colima participates and reports nothing running; the inventory
        // stays complete and empty.
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], Vec::new())),
            |_| Ok(Vec::new()),
        );

        assert!(inventory.is_complete());
        assert!(inventory.rows.is_empty());
        assert!(inventory.failure_summary().is_none());
    }

    #[test]
    fn inactive_docker_participation_skips_the_docker_probe_without_a_warning() {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(failure(
                    "runtime ps",
                    "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
                ))
            },
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], Vec::new())),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert_eq!(docker_probes.get(), 0);
        assert!(inventory.is_complete());
        assert!(inventory.failure_summary().is_none());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "effigy");
    }

    #[test]
    fn inactive_colima_participation_skips_profile_listing_without_a_warning() {
        let colima_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || Ok(Vec::new()),
            RuntimeParticipation::Inactive,
            || {
                colima_probes.set(colima_probes.get() + 1);
                Err(ContainerExecError::Launch {
                    command: "colima list --json".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
                })
            },
            |_| panic!("inactive colima profiles must not be probed"),
        );

        assert_eq!(colima_probes.get(), 0);
        assert!(inventory.is_complete());
        assert!(inventory.failure_summary().is_none());
    }

    #[test]
    fn profile_list_failure_marks_inventory_incomplete_with_actionable_context() {
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || {
                Err(ContainerExecError::Launch {
                    command: "colima list --json".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "colima not found"),
                })
            },
            |_| Ok(Vec::new()),
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.failures[0].backend, "colima");
        assert_eq!(inventory.failures[0].profile, "profile-list");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("colima not found"));
    }

    #[test]
    fn stopped_profile_is_its_own_failure_instead_of_an_incomplete_inventory() {
        // effigy#294: a stopped profile cannot be listed, but it publishes no
        // host port, so it is recorded as a per-profile fact rather than
        // poisoning every other profile's rows.
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], vec!["effigy-release".to_owned()])),
            |profile| match profile {
                "effigy" => Ok(vec![row("live-project", "/tmp/live-project")]),
                _ => unreachable!("a stopped profile is never probed"),
            },
        );

        assert_eq!(inventory.stopped_profiles(), vec!["effigy-release"]);
        assert_eq!(
            inventory.incomplete_failure_summary(),
            None,
            "a stopped profile is not an incomplete inventory"
        );
        assert!(inventory.failure_summary().is_some());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "effigy");
    }

    #[test]
    fn profile_ps_failure_does_not_discard_rows_from_other_profiles() {
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Inactive,
            || panic!("inactive docker must not be probed"),
            RuntimeParticipation::Participating,
            || Ok((vec!["broken".to_owned(), "live".to_owned()], Vec::new())),
            |profile| match profile {
                "broken" => Err(failure(
                    "colima nerdctl ps",
                    "failed to parse docker ps row",
                )),
                "live" => Ok(vec![row("live-project", "/tmp/live-project")]),
                _ => unreachable!(),
            },
        );

        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.rows[0].profile, "live");
        assert_eq!(
            inventory.rows[0].row.project_name.as_deref(),
            Some("live-project")
        );
        assert_eq!(inventory.failures[0].backend, "colima");
        assert_eq!(inventory.failures[0].profile, "broken");
    }

    #[test]
    fn active_docker_failure_is_reported_even_when_colima_inventory_succeeds() {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            RuntimeParticipation::Participating,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(ContainerExecError::Launch {
                    command: "docker ps".to_owned(),
                    error: std::io::Error::new(std::io::ErrorKind::NotFound, "docker not found"),
                })
            },
            RuntimeParticipation::Participating,
            || Ok((vec!["effigy".to_owned()], Vec::new())),
            |_| Ok(vec![row("live-project", "/tmp/live-project")]),
        );

        assert_eq!(docker_probes.get(), 1);
        assert!(!inventory.is_complete());
        assert_eq!(inventory.rows.len(), 1);
        assert_eq!(inventory.failures[0].backend, "docker");
        assert_eq!(inventory.failures[0].profile, "default");
        assert!(inventory
            .failure_summary()
            .expect("failure summary")
            .contains("docker not found"));
    }

    /// Collect with an explicit Docker verdict while counting Docker probes
    /// and injecting a connection failure.
    fn collect_with_docker_failure(
        participation: RuntimeParticipation,
    ) -> (usize, RunningComposeContainerInventory) {
        let docker_probes = Cell::new(0);
        let inventory = collect_running_compose_container_inventory(
            participation,
            || {
                docker_probes.set(docker_probes.get() + 1);
                Err(failure(
                    "runtime ps",
                    "Cannot connect to the Docker daemon at unix:///var/run/docker.sock",
                ))
            },
            RuntimeParticipation::Inactive,
            || panic!("inactive colima must not be listed"),
            |_| Ok(Vec::new()),
        );
        (docker_probes.get(), inventory)
    }

    /// Point `DOCKER_CONFIG` at an isolated fixture for the duration of `body`,
    /// restoring the previous value afterwards.
    fn with_docker_config<T>(config_dir: &Path, body: impl FnOnce() -> T) -> T {
        let _lock = crate::test_env_lock();
        let previous_config = std::env::var_os("DOCKER_CONFIG");
        let previous_host = std::env::var_os("DOCKER_HOST");
        let previous_context = std::env::var_os("DOCKER_CONTEXT");
        std::env::set_var("DOCKER_CONFIG", config_dir.as_os_str());
        std::env::remove_var("DOCKER_HOST");
        std::env::remove_var("DOCKER_CONTEXT");
        let result = body();
        match previous_config {
            Some(value) => std::env::set_var("DOCKER_CONFIG", value),
            None => std::env::remove_var("DOCKER_CONFIG"),
        }
        match previous_host {
            Some(value) => std::env::set_var("DOCKER_HOST", value),
            None => std::env::remove_var("DOCKER_HOST"),
        }
        match previous_context {
            Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
            None => std::env::remove_var("DOCKER_CONTEXT"),
        }
        result
    }

    /// A broken docker config still participates and reports the docker
    /// failure exactly: the probe runs once, the inventory is incomplete,
    /// and the single failure row names the docker backend and its error.
    fn assert_broken_config_reports_docker_failure(
        participation: RuntimeParticipation,
        what: &str,
    ) {
        assert_eq!(participation, RuntimeParticipation::Participating, "{what}");
        let (docker_probes, inventory) = collect_with_docker_failure(participation);
        assert_eq!(docker_probes, 1, "{what}: Docker is still probed");
        assert!(!inventory.is_complete(), "{what}: fail closed");
        assert_eq!(
            inventory.failures,
            [RuntimeInventoryFailure {
                backend: "docker".to_owned(),
                profile: "default".to_owned(),
                error: "runtime ps failed (exit code 1)\nstdout:\n\nstderr:\nCannot connect to the Docker daemon at unix:///var/run/docker.sock".to_owned(),
                stopped_profile: false,
            }],
            "{what}"
        );
    }

    #[test]
    fn broken_docker_configs_participate_and_report_the_docker_failure() {
        use super::super::participation::environment_participation_for_test;

        // A trailing space in the config directory still participates.
        let root = tempfile::tempdir().expect("create docker config fixture");
        let config_dir = root.path().join("config ");
        let context_dir = config_dir.join("contexts").join("meta").join("remote");
        std::fs::create_dir_all(&context_dir).expect("create context meta dir");
        std::fs::write(
            config_dir.join("config.json"),
            "{\"currentContext\": \"remote\"}",
        )
        .expect("write docker config");
        std::fs::write(
            context_dir.join("meta.json"),
            "{\"Name\": \"remote\", \"Endpoints\": {\"docker\": {\"Host\": \"tcp://10.0.0.5:2375\"}}}",
        )
        .expect("write context meta");
        let participation = with_docker_config(&config_dir, environment_participation_for_test);
        assert_broken_config_reports_docker_failure(participation, "trailing space config dir");

        // A schema-invalid config stays participating and fails closed.
        for raw in ["[1, 2, 3]", "{\"currentContext\": 42}"] {
            let root = tempfile::tempdir().expect("create docker config fixture");
            std::fs::write(root.path().join("config.json"), raw).expect("write invalid config");
            let participation = with_docker_config(root.path(), environment_participation_for_test);
            assert_broken_config_reports_docker_failure(participation, raw);
        }

        // A non-Unicode DOCKER_HOST override stays participating.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let _lock = crate::test_env_lock();
            let previous_config = std::env::var_os("DOCKER_CONFIG");
            let previous_host = std::env::var_os("DOCKER_HOST");
            let previous_context = std::env::var_os("DOCKER_CONTEXT");
            std::env::remove_var("DOCKER_CONFIG");
            std::env::set_var(
                "DOCKER_HOST",
                std::ffi::OsStr::from_bytes(b"unix:///tmp/\xff\xfe.sock"),
            );
            std::env::remove_var("DOCKER_CONTEXT");
            let participation = environment_participation_for_test();
            match previous_config {
                Some(value) => std::env::set_var("DOCKER_CONFIG", value),
                None => std::env::remove_var("DOCKER_CONFIG"),
            }
            match previous_host {
                Some(value) => std::env::set_var("DOCKER_HOST", value),
                None => std::env::remove_var("DOCKER_HOST"),
            }
            match previous_context {
                Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
                None => std::env::remove_var("DOCKER_CONTEXT"),
            }
            assert_broken_config_reports_docker_failure(participation, "non-unicode DOCKER_HOST");
        }
    }
}
