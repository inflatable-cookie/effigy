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
    Failure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
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
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let container_name = fields.next()?.trim();
            let status = fields.next().unwrap_or_default();
            let render = fields.next().unwrap_or_default().trim();
            let project = fields.next().unwrap_or_default().trim();
            let service = fields.next().unwrap_or_default().trim();
            (!container_name.is_empty() && project == project_name).then(|| {
                SharedProjectContainer {
                    container_name: container_name.to_owned(),
                    running: super::parse::compose_status_is_running(status),
                    render: (!render.is_empty()).then(|| render.to_owned()),
                    service: (!service.is_empty()).then(|| service.to_owned()),
                }
            })
        })
        .collect())
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
            compose_file_display: "direct".to_owned(),
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
    workspace_builder_deadline: Option<Instant>,
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
        let current = if let Some(original_deadline) = self.workspace_builder_deadline {
            vec![verify_workspace_artifact_builder(
                repo_root,
                policy,
                &self.row,
                deadline.min(original_deadline),
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
        Err(ContainerExecError::Failure {
            command: "inspect owned binding".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "binding unavailable: container full ID or checkout/scope/backend binding changed before effect".to_owned(),
        })
    }
}

/// Refuse a builder declaration that could activate any additional service.
pub fn verify_workspace_artifact_builder_declaration(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    for file in &policy.compose_files {
        let content = policy
            .read_compose(file)
            .map_err(|error| ContainerExecError::Failure {
                command: "read workspace builder declaration".to_owned(),
                code: None,
                stdout: String::new(),
                stderr: error.to_string(),
            })?;
        let compose: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|error| ContainerExecError::Failure {
                command: "parse workspace builder declaration".to_owned(),
                code: None,
                stdout: String::new(),
                stderr: error.to_string(),
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
            return Err(ContainerExecError::Failure {
                command: "verify workspace builder declaration".to_owned(), code: None, stdout: String::new(),
                stderr: "workspace artifact builder cannot include, extend, depend on or publish other workloads".to_owned(),
            });
        }
    }
    let services = declared_service_names(policy, &policy.compose_files)?;
    if services.len() != 1 || !services.contains("builder") {
        return Err(ContainerExecError::Failure {
            command: "verify workspace builder declaration".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "workspace artifact builder must declare only the builder service".to_owned(),
        });
    }
    Ok(())
}

/// Strict builder execution authority: explicit checkout/scope labels, a full
/// immutable identity and a writable source bind at /workspace. Unlike stack
/// reactivation this never adopts legacy unlabelled containers.
pub fn verify_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<VerifiedComposeContainer, ContainerExecError> {
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
    let mut binding = ownership.verify_inspected(repo_root, policy, row, &output.stdout)?;
    let raw: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| ContainerExecError::Launch {
            command: "inspect workspace builder".to_owned(),
            error: std::io::Error::other(error),
        })?;
    let record = raw.as_array().and_then(|records| records.first());
    let checkout = record
        .and_then(|record| record.pointer("/Config/Labels/com.effigy.checkout"))
        .and_then(|value| value.as_str());
    let mounts = record
        .and_then(|record| record.get("Mounts"))
        .and_then(|value| value.as_array());
    let workspace_mounts = mounts
        .into_iter()
        .flatten()
        .filter(|mount| {
            mount.get("Destination").and_then(|value| value.as_str()) == Some("/workspace")
        })
        .collect::<Vec<_>>();
    let shadowed = mounts.into_iter().flatten().any(|mount| {
        mount
            .get("Destination")
            .and_then(|value| value.as_str())
            .is_some_and(|destination| destination.starts_with("/workspace/"))
    });
    let bind_matches = !shadowed
        && match workspace_mounts.as_slice() {
            [mount] => {
                mount.get("Type").and_then(|value| value.as_str()) == Some("bind")
                    && mount.get("RW").and_then(|value| value.as_bool()) == Some(true)
                    && mount
                        .get("Source")
                        .and_then(|value| value.as_str())
                        .and_then(|value| Path::new(value).canonicalize().ok())
                        .as_ref()
                        == Some(&ownership.canonical)
            }
            _ => false,
        };
    let no_ports = record
        .and_then(|record| record.pointer("/NetworkSettings/Ports"))
        .and_then(|value| value.as_object())
        .is_some_and(|ports| ports.is_empty());
    // Scoped checkout labels locate the owner, while Compose may name its
    // immutable render directory. A supplied working directory must still
    // belong to this checkout or one of its retained render generations.
    let working_dir_matches = builder_working_directory_is_owned(
        &ownership.canonical,
        record.and_then(|record| {
            record.pointer("/Config/Labels/com.docker.compose.project.working_dir")
        }),
    );
    if !ownership.owns(checkout, binding.scope.as_deref())
        || !bind_matches
        || !no_ports
        || !working_dir_matches
    {
        return Err(ContainerExecError::Failure {
            command: "inspect workspace builder".to_owned(), code: None, stdout: String::new(),
            stderr: "workspace builder checkout/scope/working-directory labels or writable /workspace source bind are unverified".to_owned(),
        });
    }
    binding.workspace_builder_deadline = Some(deadline);
    Ok(binding)
}

fn builder_working_directory_is_owned(checkout: &Path, label: Option<&serde_json::Value>) -> bool {
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
                && super::parse::compose_status_is_running(&row.status)
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
        Ok(Self {
            declared,
            canonical,
            scope_token,
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
        scope_matches
            && checkout
                .and_then(|path| Path::new(path).canonicalize().ok())
                .as_ref()
                == Some(&self.canonical)
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
        let refuse = |reason: &str| ContainerExecError::Failure {
            command: "list owned service".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: format!("binding unavailable: {reason}"),
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
        let raw: serde_json::Value =
            serde_json::from_slice(inspect_stdout).map_err(|_| ContainerExecError::Failure {
                command: "inspect owned binding".to_owned(),
                code: None,
                stdout: String::new(),
                stderr: "binding unavailable: inspect JSON is invalid".to_owned(),
            })?;
        let identity = super::parse::parse_inspected_container_identity(&String::from_utf8_lossy(
            inspect_stdout,
        ))
        .map_err(|_| ContainerExecError::Failure {
            command: "inspect owned binding".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "binding unavailable: canonical full container identity is missing".to_owned(),
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
            return Err(ContainerExecError::Failure { command: "inspect owned binding".to_owned(), code: None, stdout: String::new(), stderr: "binding unavailable: full-ID/project/service/oneoff/running/checkout ownership changed or is unknown".to_owned() });
        }
        let Some(ports) = record
            .and_then(|record| record.get("NetworkSettings"))
            .and_then(|network| network.get("Ports"))
            .and_then(|ports| ports.as_object())
        else {
            return Err(ContainerExecError::Failure {
                command: "inspect owned binding".to_owned(),
                code: None,
                stdout: String::new(),
                stderr: "binding unavailable: fresh published-port associations are missing"
                    .to_owned(),
            });
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
                        current.ports.push(format!("{ip}:{port}->{container_port}"));
                    }
                }
            }
        }
        Ok(VerifiedComposeContainer {
            workspace_builder_deadline: None,
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
            status: "Up 10 seconds".to_owned(),
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

    #[test]
    fn inactive_optional_runtime_never_poisons_a_complete_empty_inventory() {
        // The Colima-only shape: Docker Desktop is installed but proven
        // inactive, Colima participates and reports nothing running.
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

    #[test]
    fn trailing_space_config_directory_participates_and_reports_docker_failure() {
        use super::super::participation::environment_participation_for_test;

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
        assert_eq!(participation, RuntimeParticipation::Participating);

        let (docker_probes, inventory) = collect_with_docker_failure(participation);
        assert_eq!(docker_probes, 1);
        assert!(!inventory.is_complete());
        assert_eq!(inventory.failures[0].backend, "docker");
        assert!(inventory
            .failure_summary()
            .expect("docker failure")
            .contains("Cannot connect to the Docker daemon"));
    }

    #[test]
    fn schema_invalid_config_participates_and_reports_docker_failure() {
        use super::super::participation::environment_participation_for_test;

        for raw in ["[1, 2, 3]", "{\"currentContext\": 42}"] {
            let root = tempfile::tempdir().expect("create docker config fixture");
            std::fs::write(root.path().join("config.json"), raw).expect("write invalid config");

            let participation = with_docker_config(root.path(), environment_participation_for_test);
            assert_eq!(
                participation,
                RuntimeParticipation::Participating,
                "invalid config must stay participating: {raw}"
            );

            let (docker_probes, inventory) = collect_with_docker_failure(participation);
            assert_eq!(
                docker_probes, 1,
                "invalid config must still probe Docker: {raw}"
            );
            assert!(
                !inventory.is_complete(),
                "invalid config must fail closed: {raw}"
            );
            assert!(inventory
                .failure_summary()
                .expect("docker failure")
                .contains("Cannot connect to the Docker daemon"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_docker_override_participates_and_reports_docker_failure() {
        use super::super::participation::environment_participation_for_test;
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

        assert_eq!(participation, RuntimeParticipation::Participating);

        let (docker_probes, inventory) = collect_with_docker_failure(participation);
        assert_eq!(docker_probes, 1);
        assert!(!inventory.is_complete());
        assert!(inventory
            .failure_summary()
            .expect("docker failure")
            .contains("Cannot connect to the Docker daemon"));
    }
}
