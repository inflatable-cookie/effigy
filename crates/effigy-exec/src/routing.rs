//! Execution routing decisions.
//!
//! Given the project's execution context and what the selected task declares,
//! determines whether the task runs on the host or inside a container, and
//! which container/service to target. The caller has already selected the
//! task; routing never looks at the task's name.
//!
//! ## Routing rules
//!
//! 1. If no default execution container is resolved → host.
//! 2. If the task declares the host (`run_in = "host"` or a host binding) → host.
//! 3. If the task names a container (`run_in = "container"` or a named
//!    binding) → that container.
//! 4. Otherwise (`run_in = "either"`) → the default container.

/// Describes the project's execution context configuration.
#[derive(Debug, Clone)]
pub struct ExecContext {
    /// Name of the resolved default execution container, if any.
    pub default_container: Option<String>,

    /// The primary service in the default execution container.
    pub primary_service: Option<String>,

    /// Whether the default execution container is currently running.
    pub container_running: bool,
}

impl ExecContext {
    /// Create a context with no default execution container.
    pub fn none() -> Self {
        Self {
            default_container: None,
            primary_service: None,
            container_running: false,
        }
    }

    /// Create a context with a default execution container.
    pub fn with_container(
        name: impl Into<String>,
        primary_service: impl Into<String>,
        running: bool,
    ) -> Self {
        Self {
            default_container: Some(name.into()),
            primary_service: Some(primary_service.into()),
            container_running: running,
        }
    }
}

/// Where the selected task's own declaration sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredTarget {
    /// No declared target: the default execution container applies.
    Default,
    /// The task is declared or bound to the host.
    Host,
    /// The task names this container.
    Container(String),
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

/// Determine where the selected task should execute.
pub fn route(context: &ExecContext, declared: &DeclaredTarget) -> RoutingDecision {
    // 1. No default execution container → always host.
    let (container, service) = match (&context.default_container, &context.primary_service) {
        (Some(c), Some(s)) => (c.clone(), s.clone()),
        _ => return RoutingDecision::host("no default execution container resolved"),
    };

    // 2. The task is declared or bound to the host.
    let named = match declared {
        DeclaredTarget::Host => {
            return RoutingDecision::host("task declares run_in = \"host\"");
        }
        DeclaredTarget::Container(named) => Some(named),
        DeclaredTarget::Default => None,
    };

    // 3. The task names a container.
    if let Some(container) = named {
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

    // 4. Default → default execution container.
    if !context.container_running {
        return RoutingDecision::not_running(
            container,
            "default execution container is not running",
        );
    }

    RoutingDecision::container(container, service, "routed to default execution container")
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
