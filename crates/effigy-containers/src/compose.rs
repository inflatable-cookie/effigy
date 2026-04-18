//! Docker Compose backend resolution and command building.
//!
//! Extracted from `src/runner/container_command.rs` — these are
//! container-domain decisions about how to invoke Docker/Colima,
//! not runner shell behavior.

use std::ffi::OsString;

use crate::EffectiveContainerPolicy;

use effigy_manifest::{ManifestContainerOnTaskExit, ManifestContainerShutdownMode};

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
/// Prefers `docker` if available on PATH, falls back to Colima nerdctl.
pub fn resolve_compose_backend() -> ComposeBackend {
    if command_exists("docker") {
        ComposeBackend::Docker
    } else {
        ComposeBackend::ColimaNerdctl
    }
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

/// Resolve the final program and arguments for a compose invocation.
///
/// For `ComposeBackend::Docker`, returns `("docker", args)`.
/// For `ComposeBackend::ColimaNerdctl`, wraps in `colima nerdctl --profile <p> -- <args>`.
pub fn compose_invocation(
    policy: &EffectiveContainerPolicy,
    args: &[OsString],
) -> (&'static str, Vec<OsString>) {
    match resolve_compose_backend() {
        ComposeBackend::Docker => ("docker", args.to_vec()),
        ComposeBackend::ColimaNerdctl => {
            let mut resolved = vec![
                OsString::from("nerdctl"),
                OsString::from("--profile"),
                OsString::from(policy.profile.as_str()),
                OsString::from("--"),
            ];
            resolved.extend(args.iter().cloned());
            ("colima", resolved)
        }
    }
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

fn command_exists(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|entry| entry.join(program).is_file()))
}

#[cfg(test)]
#[path = "compose/tests.rs"]
mod tests;
