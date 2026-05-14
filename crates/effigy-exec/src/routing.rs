//! Execution routing decisions.
//!
//! Given a command and the project's execution context, determines whether
//! the command should run on the host or inside a container, and which
//! container/service to target.
//!
//! ## Routing rules
//!
//! 1. If no dev-context container is declared → host (current behavior).
//! 2. If the command is a host-native built-in → host.
//! 3. If the task explicitly declares `run_in = "host"` → host.
//! 4. If the task explicitly declares `run_in = "container"` or binds to a
//!    specific container → that container.
//! 5. Otherwise (`run_in = "either"`) → the current/default context.

/// Describes the project's execution context configuration.
#[derive(Debug, Clone)]
pub struct ExecContext {
    /// Name of the container with `context = "dev"`, if any.
    pub dev_container: Option<String>,

    /// The primary service in the dev container (for exec targeting).
    pub primary_service: Option<String>,

    /// Whether the dev container is currently running.
    pub container_running: bool,
}

impl ExecContext {
    /// Create a context with no dev container (all commands run on host).
    pub fn none() -> Self {
        Self {
            dev_container: None,
            primary_service: None,
            container_running: false,
        }
    }

    /// Create a context with a dev container.
    pub fn with_container(
        name: impl Into<String>,
        primary_service: impl Into<String>,
        running: bool,
    ) -> Self {
        Self {
            dev_container: Some(name.into()),
            primary_service: Some(primary_service.into()),
            container_running: running,
        }
    }
}

/// Per-task execution overrides.
#[derive(Debug, Clone, Default)]
pub struct TaskOverrides {
    /// If true, force host execution regardless of context.
    pub host: bool,

    /// Explicit container target override.
    pub container: Option<String>,
}

/// The result of a routing decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecTarget {
    /// Run on the host.
    Host,

    /// Run inside the specified container/service.
    Container {
        /// Container name.
        container: String,
        /// Service to exec into.
        service: String,
    },

    /// The command should run in the container but it's not running.
    /// The caller should prompt the user to start it.
    ContainerNotRunning {
        /// Container name.
        container: String,
    },
}

/// Commands that always run on the host (effigy's own management surface).
const HOST_NATIVE_COMMANDS: &[&str] = &[
    "doctor",
    "container",
    "gateway",
    "service",
    "catalog",
    "release",
    "tasks",
    "help",
    "version",
    "init",
    "migrate",
    "bootstrap",
    "distribution",
];

/// Determine where a command should execute.
///
/// `command_name` is the first argument to effigy (e.g., "test", "doctor",
/// "exec"). `task_overrides` contains any per-task routing directives.
pub fn route(
    command_name: &str,
    context: &ExecContext,
    task_overrides: &TaskOverrides,
) -> RoutingDecision {
    // 1. No dev context → always host.
    let (container, service) = match (&context.dev_container, &context.primary_service) {
        (Some(c), Some(s)) => (c.clone(), s.clone()),
        _ => return RoutingDecision::host("no dev-context container declared"),
    };

    // 2. Host-native commands always run on host.
    if HOST_NATIVE_COMMANDS.contains(&command_name) {
        return RoutingDecision::host(format!("'{command_name}' is a host-native command"));
    }

    // 3. Explicit `run_in = "host"` override.
    if task_overrides.host {
        return RoutingDecision::host("task declares run_in = \"host\"");
    }

    // 4. Explicit container target override.
    if let Some(ref container) = task_overrides.container {
        return if context.container_running {
            RoutingDecision::container(
                container.clone(),
                service.clone(),
                format!("task declares explicit container = \"{container}\""),
            )
        } else {
            RoutingDecision::not_running(
                container.clone(),
                format!("container '{container}' is not running"),
            )
        };
    }

    // 6. Default → dev-context container.
    if !context.container_running {
        return RoutingDecision::not_running(container, "dev-context container is not running");
    }

    RoutingDecision::container(container, service, "routed to dev-context container")
}

/// The full routing decision with explanation.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Where the command should execute.
    pub target: ExecTarget,

    /// Human-readable explanation of why this decision was made.
    pub reason: String,
}

impl RoutingDecision {
    /// Create a host routing decision.
    pub fn host(reason: impl Into<String>) -> Self {
        Self {
            target: ExecTarget::Host,
            reason: reason.into(),
        }
    }

    /// Create a container routing decision.
    pub fn container(
        container: impl Into<String>,
        service: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            target: ExecTarget::Container {
                container: container.into(),
                service: service.into(),
            },
            reason: reason.into(),
        }
    }

    /// Create a not-running routing decision.
    pub fn not_running(container: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            target: ExecTarget::ContainerNotRunning {
                container: container.into(),
            },
            reason: reason.into(),
        }
    }

    /// Whether this routes to a container.
    pub fn is_container(&self) -> bool {
        matches!(self.target, ExecTarget::Container { .. })
    }

    /// Whether this routes to the host.
    pub fn is_host(&self) -> bool {
        matches!(self.target, ExecTarget::Host)
    }

    /// Whether the target container is not running.
    pub fn is_not_running(&self) -> bool {
        matches!(self.target, ExecTarget::ContainerNotRunning { .. })
    }
}

#[cfg(test)]
#[path = "routing/tests.rs"]
mod tests;
