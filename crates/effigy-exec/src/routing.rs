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
//! 3. If the task explicitly declares `host = true` → host.
//! 4. If the task explicitly declares `container_session = "none"` → host.
//! 5. If the task declares `container_session = "<name>"` → that container.
//! 6. Otherwise → the dev-context container.

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

    /// Explicit container session override. "none" means host.
    pub container_session: Option<String>,
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

    // 3. Explicit `host = true` override.
    if task_overrides.host {
        return RoutingDecision::host("task declares host = true");
    }

    // 4. Explicit `container_session = "none"`.
    if let Some(ref session) = task_overrides.container_session {
        if session == "none" {
            return RoutingDecision::host("task declares container_session = \"none\"");
        }
        // 5. Explicit container session targeting a different container.
        return if context.container_running {
            RoutingDecision::container(
                session.clone(),
                service.clone(),
                format!("task declares container_session = \"{session}\""),
            )
        } else {
            RoutingDecision::not_running(
                session.clone(),
                format!("container '{session}' is not running"),
            )
        };
    }

    // 6. Default → dev-context container.
    if !context.container_running {
        return RoutingDecision::not_running(
            container,
            "dev-context container is not running",
        );
    }

    RoutingDecision::container(
        container,
        service,
        "routed to dev-context container",
    )
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
mod tests {
    use super::*;

    fn running_context() -> ExecContext {
        ExecContext::with_container("web", "app", true)
    }

    fn stopped_context() -> ExecContext {
        ExecContext::with_container("web", "app", false)
    }

    fn no_context() -> ExecContext {
        ExecContext::none()
    }

    fn no_overrides() -> TaskOverrides {
        TaskOverrides::default()
    }

    // ── No context ───────────────────────────────────────────────────

    #[test]
    fn no_context_routes_to_host() {
        let d = route("test", &no_context(), &no_overrides());
        assert!(d.is_host());
    }

    #[test]
    fn no_context_even_for_tasks() {
        let d = route("build", &no_context(), &no_overrides());
        assert!(d.is_host());
    }

    // ── Host-native commands ─────────────────────────────────────────

    #[test]
    fn doctor_always_host() {
        let d = route("doctor", &running_context(), &no_overrides());
        assert!(d.is_host());
        assert!(d.reason.contains("host-native"));
    }

    #[test]
    fn container_command_always_host() {
        let d = route("container", &running_context(), &no_overrides());
        assert!(d.is_host());
    }

    #[test]
    fn gateway_always_host() {
        let d = route("gateway", &running_context(), &no_overrides());
        assert!(d.is_host());
    }

    #[test]
    fn release_always_host() {
        let d = route("release", &running_context(), &no_overrides());
        assert!(d.is_host());
    }

    #[test]
    fn tasks_always_host() {
        let d = route("tasks", &running_context(), &no_overrides());
        assert!(d.is_host());
    }

    #[test]
    fn help_always_host() {
        let d = route("help", &running_context(), &no_overrides());
        assert!(d.is_host());
    }

    #[test]
    fn version_always_host() {
        let d = route("version", &running_context(), &no_overrides());
        assert!(d.is_host());
    }

    // ── Task overrides ───────────────────────────────────────────────

    #[test]
    fn host_override_forces_host() {
        let overrides = TaskOverrides {
            host: true,
            ..Default::default()
        };
        let d = route("test", &running_context(), &overrides);
        assert!(d.is_host());
        assert!(d.reason.contains("host = true"));
    }

    #[test]
    fn container_session_none_forces_host() {
        let overrides = TaskOverrides {
            container_session: Some("none".to_string()),
            ..Default::default()
        };
        let d = route("test", &running_context(), &overrides);
        assert!(d.is_host());
    }

    #[test]
    fn container_session_targets_specific_container() {
        let overrides = TaskOverrides {
            container_session: Some("staging".to_string()),
            ..Default::default()
        };
        let d = route("test", &running_context(), &overrides);
        assert!(d.is_container());
        if let ExecTarget::Container { container, .. } = &d.target {
            assert_eq!(container, "staging");
        }
    }

    // ── Default routing ──────────────────────────────────────────────

    #[test]
    fn task_routes_to_dev_container() {
        let d = route("test", &running_context(), &no_overrides());
        assert!(d.is_container());
        if let ExecTarget::Container {
            container, service, ..
        } = &d.target
        {
            assert_eq!(container, "web");
            assert_eq!(service, "app");
        }
    }

    #[test]
    fn custom_task_routes_to_dev_container() {
        let d = route("seed:fresh", &running_context(), &no_overrides());
        assert!(d.is_container());
    }

    #[test]
    fn exec_routes_to_dev_container() {
        let d = route("exec", &running_context(), &no_overrides());
        assert!(d.is_container());
    }

    // ── Container not running ────────────────────────────────────────

    #[test]
    fn stopped_container_returns_not_running() {
        let d = route("test", &stopped_context(), &no_overrides());
        assert!(d.is_not_running());
        if let ExecTarget::ContainerNotRunning { container } = &d.target {
            assert_eq!(container, "web");
        }
    }

    #[test]
    fn stopped_container_with_session_override() {
        let overrides = TaskOverrides {
            container_session: Some("staging".to_string()),
            ..Default::default()
        };
        let d = route("test", &stopped_context(), &overrides);
        assert!(d.is_not_running());
    }

    // ── Decision metadata ────────────────────────────────────────────

    #[test]
    fn decision_has_reason() {
        let d = route("test", &running_context(), &no_overrides());
        assert!(!d.reason.is_empty());
    }

    #[test]
    fn host_native_reason_mentions_command() {
        let d = route("doctor", &running_context(), &no_overrides());
        assert!(d.reason.contains("doctor"));
    }
}
