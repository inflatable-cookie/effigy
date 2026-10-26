//! Container capability detection.
//!
//! Probes a running container to determine:
//!
//! - Whether effigy is installed inside the container (for handoff)
//! - Which shell is available (bash, sh, etc.)
//! - Whether specific tools are available (composer, npm, etc.)
//!
//! Detection results are cached per container session to avoid repeated
//! probes during a single development session.

use std::collections::HashMap;
use std::time::Instant;

/// Result of probing a container for capabilities.
#[derive(Debug, Clone)]
pub struct ContainerCapabilities {
    /// Whether effigy is installed inside the container.
    pub has_effigy: bool,

    /// Effigy version inside the container, if installed.
    pub effigy_version: Option<String>,

    /// Available shell (first match from preferred list).
    pub shell: String,

    /// Map of tool names to their availability.
    pub tools: HashMap<String, bool>,

    /// When this probe was performed.
    pub probed_at: Instant,
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

impl ExecStrategy {
    /// Build the docker compose exec command arguments.
    ///
    /// Returns the arguments to append after `docker compose exec <service>`.
    pub fn compose_exec_args(&self) -> Vec<String> {
        match self {
            Self::Handoff { args } => {
                let mut result = vec!["effigy".to_string()];
                result.extend(args.iter().cloned());
                result
            }
            Self::RawExec {
                working_dir,
                command,
            } => {
                let mut result = vec!["-w".to_string(), working_dir.clone()];
                result.extend(command.iter().cloned());
                result
            }
        }
    }
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
