//! Docker Compose backend resolution and command building.
//!
//! Extracted from `src/runner/container_command.rs` — these are
//! container-domain decisions about how to invoke Docker/Colima,
//! not runner shell behavior.

#[cfg(test)]
use std::cell::Cell;
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

#[cfg(test)]
thread_local! {
    static TEST_COMPOSE_BACKEND_OVERRIDE: Cell<Option<ComposeBackend>> = const { Cell::new(None) };
}

const COMPOSE_BACKEND_OVERRIDE_ENV: &str = "EFFIGY_COMPOSE_BACKEND";

/// Resolve which compose backend to use.
///
/// Prefers `docker` if available on PATH, falls back to Colima nerdctl.
pub fn resolve_compose_backend() -> ComposeBackend {
    #[cfg(test)]
    if let Some(backend) = TEST_COMPOSE_BACKEND_OVERRIDE.with(Cell::get) {
        return backend;
    }
    if let Some(backend) = compose_backend_override_from_env() {
        return backend;
    }
    if command_exists("docker") {
        ComposeBackend::Docker
    } else {
        ComposeBackend::ColimaNerdctl
    }
}

#[cfg(test)]
pub(crate) fn with_test_compose_backend<T>(backend: ComposeBackend, run: impl FnOnce() -> T) -> T {
    TEST_COMPOSE_BACKEND_OVERRIDE.with(|slot| {
        let previous = slot.replace(Some(backend));
        let result = run();
        slot.set(previous);
        result
    })
}

fn compose_backend_override_from_env() -> Option<ComposeBackend> {
    match std::env::var(COMPOSE_BACKEND_OVERRIDE_ENV)
        .ok()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "auto" => None,
        "docker" => Some(ComposeBackend::Docker),
        "colima" | "colima-nerdctl" | "nerdctl" | "containerd" => {
            Some(ComposeBackend::ColimaNerdctl)
        }
        _ => None,
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
