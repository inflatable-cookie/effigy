//! Container capability detection.
//!
//! Probes a running container to determine:
//!
//! - Whether effigy is installed inside the container (for handoff)
//! - Which shell is available (bash, sh)
//! - Which effigy version the container reports
//!
//! A probe is identified by [`ProbeKind`] — never by its human description — so
//! the result a caller records is the same value the builder reads back, and
//! each kind owns how its result is interpreted.

use std::collections::HashMap;

/// Result of probing a container for capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCapabilities {
    /// Whether effigy is installed inside the container.
    pub has_effigy: bool,

    /// Effigy version inside the container, if a version probe reported one.
    pub effigy_version: Option<String>,

    /// Available shell, when the container offered one. A container that
    /// answered neither shell probe claims no shell; it is never reported as
    /// `/bin/sh` by default.
    pub shell: Option<String>,
}

impl ContainerCapabilities {
    /// Whether the container supports effigy handoff.
    ///
    /// Handoff means the host effigy invokes the container effigy directly,
    /// letting it handle path resolution, environment, and task execution
    /// natively.
    pub fn supports_handoff(&self) -> bool {
        self.has_effigy
    }
}

/// Strategy for executing a command inside a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecStrategy {
    /// Hand off to effigy inside the container.
    ///
    /// The host effigy runs:
    /// `docker compose exec <service> effigy <args...>`
    ///
    /// The container effigy handles everything natively — no CWD mapping,
    /// no path translation, full access to the runtime environment.
    Handoff {
        /// The effigy command and arguments to run in the container.
        args: Vec<String>,
    },

    /// Execute a raw command inside the container.
    ///
    /// The host effigy runs:
    /// `docker compose exec -w <cwd> <service> <command...>`
    ///
    /// CWD mapping and environment setup are handled by the host.
    RawExec {
        /// The container-side working directory.
        working_dir: String,
        /// The command and arguments to run.
        command: Vec<String>,
    },
}

/// Determine the execution strategy for a task invocation.
///
/// If the container has effigy installed, hands off the entire command.
/// Otherwise, falls back to raw exec with CWD mapping.
pub fn determine_strategy(
    capabilities: &ContainerCapabilities,
    task_name: &str,
    task_args: &[String],
    container_cwd: &str,
    raw_command: &[String],
) -> ExecStrategy {
    if capabilities.supports_handoff() {
        // Hand off to the container's effigy.
        let mut args = vec![task_name.to_string()];
        args.extend(task_args.iter().cloned());
        ExecStrategy::Handoff { args }
    } else {
        // Raw exec with CWD mapping.
        ExecStrategy::RawExec {
            working_dir: container_cwd.to_string(),
            command: raw_command.to_vec(),
        }
    }
}

/// How a probe result is interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeInterpreter {
    /// The exit status decides; stdout carries no capability fact.
    ExitCode,

    /// The exit status decides, and stdout carries the reported version.
    VersionOutput,
}

impl ProbeInterpreter {
    /// The version a probe reported, if its own interpreter reads output.
    fn reported_version(self, output: &str) -> Option<String> {
        match self {
            Self::ExitCode => None,
            Self::VersionOutput => {
                let output = output.trim();
                (!output.is_empty()).then(|| output.to_owned())
            }
        }
    }
}

/// The identity of one capability probe: how its result is recorded and read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeKind {
    /// Whether effigy is installed inside the container.
    EffigyInstallation,
    /// Which effigy version the container reports.
    EffigyVersion,
    /// Whether bash is available.
    Bash,
    /// Whether sh is available.
    Sh,
}

impl ProbeKind {
    /// How this check's result is interpreted.
    pub fn interpreter(self) -> ProbeInterpreter {
        match self {
            Self::EffigyInstallation | Self::Bash | Self::Sh => ProbeInterpreter::ExitCode,
            Self::EffigyVersion => ProbeInterpreter::VersionOutput,
        }
    }
}

/// Probe specification for detecting container capabilities.
///
/// This struct describes what to probe and how. The actual probing
/// (running docker compose exec) is performed by the caller — this
/// crate provides the logic, not the I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeSpec {
    /// Checks to run, each identified by the kind whose result it produces.
    pub checks: Vec<ProbeCheck>,
}

/// A single probe check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeCheck {
    /// Which capability this check reports.
    pub kind: ProbeKind,

    /// Command to run inside the container (via docker compose exec).
    pub command: Vec<String>,
}

/// Build the standard probe spec for a container.
///
/// Returns the set of checks needed to determine capabilities. The
/// caller runs these checks and feeds the results back to
/// `build_capabilities_from_results`.
pub fn standard_probe_spec() -> ProbeSpec {
    ProbeSpec {
        checks: vec![
            ProbeCheck {
                kind: ProbeKind::EffigyInstallation,
                command: vec!["which".to_string(), "effigy".to_string()],
            },
            ProbeCheck {
                kind: ProbeKind::EffigyVersion,
                command: vec!["effigy".to_string(), "version".to_string()],
            },
            ProbeCheck {
                kind: ProbeKind::Bash,
                command: vec!["which".to_string(), "bash".to_string()],
            },
            ProbeCheck {
                kind: ProbeKind::Sh,
                command: vec!["which".to_string(), "sh".to_string()],
            },
        ],
    }
}

/// Result of running a single probe check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    /// Whether the command exited successfully (exit code 0).
    pub success: bool,

    /// Stdout output (trimmed).
    pub output: String,
}

/// Build container capabilities from probe results.
///
/// `results` is keyed by the [`ProbeKind`] each check reports, so every check in
/// [`standard_probe_spec`] is read back by its own identity.
pub fn build_capabilities_from_results(
    results: &HashMap<ProbeKind, ProbeResult>,
) -> ContainerCapabilities {
    let answered = |kind: ProbeKind| results.get(&kind).filter(|result| result.success);

    let effigy_version = answered(ProbeKind::EffigyVersion).and_then(|result| {
        ProbeKind::EffigyVersion
            .interpreter()
            .reported_version(&result.output)
    });

    let shell = if answered(ProbeKind::Bash).is_some() {
        Some("/bin/bash".to_string())
    } else {
        answered(ProbeKind::Sh).map(|_| "/bin/sh".to_string())
    };

    ContainerCapabilities {
        has_effigy: answered(ProbeKind::EffigyInstallation).is_some(),
        effigy_version,
        shell,
    }
}

#[cfg(test)]
#[path = "detection/tests.rs"]
mod tests;
