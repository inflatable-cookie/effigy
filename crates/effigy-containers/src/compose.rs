//! Docker Compose backend resolution and command building.
//!
//! Extracted from `src/runner/container_command.rs` — these are
//! container-domain decisions about how to invoke Docker/Colima,
//! not runner shell behavior.

use std::ffi::OsString;

use crate::EffectiveContainerPolicy;

use effigy_container_manager::{
    resolve_host_cli_program as manager_resolve_host_cli_program, BackendId,
    ContainerBackendDetection, ContainerBackendRegistry,
};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
};

/// The backend used to run Docker Compose commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeBackend {
    /// Docker CLI with compose plugin (`docker compose`).
    Docker,
    /// Colima nerdctl fallback (`colima nerdctl -- compose`).
    ColimaNerdctl,
}

/// Resolve which compose backend to use.
///
/// Precedence:
/// - explicit env override
/// - user-global backend preference
/// - ambient runtime detection
pub fn resolve_compose_backend() -> ComposeBackend {
    #[cfg(test)]
    if let Some(backend) = tests::test_compose_backend_override() {
        return backend;
    }
    let detection = compose_backend_detection();
    let backend_id = ContainerBackendRegistry::defaults()
        .detect_backend(&detection)
        .unwrap_or_else(|_| BackendId::colima_nerdctl());
    ComposeBackend::from_backend_id(&backend_id)
}

pub fn resolve_compose_backend_for_policy(policy: &EffectiveContainerPolicy) -> ComposeBackend {
    let detection = compose_backend_detection_for_policy(policy);
    let backend_id = ContainerBackendRegistry::defaults()
        .detect_backend(&detection)
        .unwrap_or_else(|_| backend_id_for_policy(policy));
    ComposeBackend::from_backend_id(&backend_id)
}

#[cfg(test)]
pub(crate) fn with_test_compose_backend<T>(backend: ComposeBackend, run: impl FnOnce() -> T) -> T {
    tests::with_test_compose_backend(backend, run)
}

/// Build docker compose arguments for a given policy and subcommand.
///
/// Prepends `compose -f <file>... -p <project>` before the caller's args.
pub fn compose_args<'a>(
    policy: &EffectiveContainerPolicy,
    tail: impl IntoIterator<Item = &'a str>,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("compose")];
    for compose_file in &policy.compose_files {
        args.push(OsString::from("-f"));
        args.push(compose_file.as_os_str().to_os_string());
    }
    args.push(OsString::from("-p"));
    args.push(OsString::from(policy.project_name.as_str()));
    args.extend(tail.into_iter().map(OsString::from));
    args
}

pub fn normalize_compose_command_args(
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> Vec<OsString> {
    match args.first().map(|value| value.to_string_lossy()) {
        Some(first) if first == "compose" => args.to_vec(),
        _ => {
            let mut normalized = vec![OsString::from("compose")];
            for compose_file in &policy.compose_files {
                normalized.push(OsString::from("-f"));
                normalized.push(compose_file.as_os_str().to_os_string());
            }
            normalized.push(OsString::from("-p"));
            normalized.push(OsString::from(policy.project_name.as_str()));
            normalized.extend(args.iter().cloned());
            normalized
        }
    }
}

/// Build standard detached bring-up args.
///
/// Uses `--build` so compose-backed services track local Dockerfile changes
/// instead of silently reusing stale tagged images. Generated compose stacks
/// also force recreation because some backends keep existing containers on the
/// previous image even after a successful rebuild of the same local tag.
pub fn compose_up_args(policy: &EffectiveContainerPolicy) -> Vec<OsString> {
    let mut args = vec!["up", "-d", "--build"];
    if policy.compose_source == crate::EffectiveComposeSource::Generated {
        args.push("--force-recreate");
    }
    compose_args(policy, args)
}

/// Resolve the final program and arguments for a compose invocation.
///
/// For `ComposeBackend::Docker`, returns `("docker", args)`.
/// For `ComposeBackend::ColimaNerdctl`, wraps in `colima nerdctl --profile <p> -- <args>`.
pub fn compose_invocation(
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> (&'static str, Vec<OsString>) {
    let normalized_args = normalize_compose_command_args(policy, args);
    let detection = compose_backend_detection_for_policy(policy);
    let (program, resolved_args) = effigy_container_manager::ContainerManager::defaults()
        .compose_process_invocation(&detection, policy.profile.as_str(), &normalized_args)
        .unwrap_or_else(|_| {
            (
                OsString::from("colima"),
                colima_nerdctl_args(policy, &normalized_args),
            )
        });
    let program = if program == "docker" {
        "docker"
    } else {
        "colima"
    };
    (program, resolved_args)
}

/// Human-readable label for a shutdown mode.
pub fn shutdown_label(mode: ManifestContainerShutdownMode) -> &'static str {
    match mode {
        ManifestContainerShutdownMode::Graceful => "graceful",
        ManifestContainerShutdownMode::Immediate => "immediate",
    }
}

/// Human-readable label for the on_task_exit policy.
pub fn on_task_exit_label(mode: ManifestContainerOnTaskExit) -> &'static str {
    match mode {
        ManifestContainerOnTaskExit::Stop => "stop",
        ManifestContainerOnTaskExit::LeaveRunning => "leave-running",
    }
}

pub fn resolve_host_cli_program(program: &str) -> OsString {
    manager_resolve_host_cli_program(program)
}

fn colima_nerdctl_args(policy: &EffectiveContainerPolicy, args: &[OsString]) -> Vec<OsString> {
    let mut resolved = vec![
        OsString::from("nerdctl"),
        OsString::from("--profile"),
        OsString::from(policy.profile.as_str()),
        OsString::from("--"),
    ];
    resolved.extend(args.iter().cloned());
    resolved
}

#[cfg(not(test))]
fn compose_backend_detection() -> ContainerBackendDetection {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override = crate::user_global_backend_preference();
    }
    detection
}

#[cfg(test)]
fn compose_backend_detection() -> ContainerBackendDetection {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override = crate::user_global_backend_preference();
    }
    if let Some(backend) = tests::test_compose_backend_override() {
        detection.backend_override = Some(backend.backend_id());
    }
    detection
}

fn compose_backend_detection_for_policy(
    policy: &EffectiveContainerPolicy,
) -> ContainerBackendDetection {
    let mut detection = compose_backend_detection();
    if detection.backend_override.is_none() {
        detection.backend_override = Some(backend_id_for_policy(policy));
    }
    detection
}

fn backend_id_for_policy(policy: &EffectiveContainerPolicy) -> BackendId {
    match policy.driver {
        ManifestContainerDriver::Colima => BackendId::colima_nerdctl(),
    }
}

impl ComposeBackend {
    #[cfg(test)]
    fn backend_id(self) -> BackendId {
        match self {
            Self::Docker => BackendId::docker_compose(),
            Self::ColimaNerdctl => BackendId::colima_nerdctl(),
        }
    }

    fn from_backend_id(value: &BackendId) -> Self {
        if *value == BackendId::docker_compose() {
            Self::Docker
        } else {
            Self::ColimaNerdctl
        }
    }
}

#[cfg(test)]
#[path = "compose/tests.rs"]
mod tests;
