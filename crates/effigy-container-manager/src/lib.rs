use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendId(String);

impl BackendId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn docker_compose() -> Self {
        Self::new("docker-compose")
    }

    pub fn colima_nerdctl() -> Self {
        Self::new("colima-nerdctl")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerInterruptPolicy {
    Ignore,
    Forward,
    ShutdownOnInterrupt,
}

impl Default for ContainerInterruptPolicy {
    fn default() -> Self {
        Self::Forward
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerManagerRequest {
    pub repo_root: PathBuf,
    pub backend_override: Option<BackendId>,
    pub interrupt_policy: ContainerInterruptPolicy,
}

impl ContainerManagerRequest {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            backend_override: None,
            interrupt_policy: ContainerInterruptPolicy::default(),
        }
    }

    pub fn backend_override(mut self, backend: BackendId) -> Self {
        self.backend_override = Some(backend);
        self
    }

    pub fn interrupt_policy(mut self, policy: ContainerInterruptPolicy) -> Self {
        self.interrupt_policy = policy;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerActivationRequest {
    pub services: Vec<String>,
    pub attach: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerShellRequest {
    pub service: Option<String>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerExecRequest {
    pub service: Option<String>,
    pub command: Vec<String>,
    pub workdir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerShutdownRequest {
    pub remove_orphans: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntimeState {
    Unknown,
    Stopped,
    Starting,
    Running,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerOperationReport {
    pub backend_id: BackendId,
    pub policy_name: String,
    pub repo_root: PathBuf,
    pub action: ContainerAction,
    pub cleanup_result: Option<ContainerCleanupResult>,
    pub state: ContainerRuntimeState,
    pub notes: Vec<String>,
}

impl ContainerOperationReport {
    pub fn new(
        backend_id: BackendId,
        repo_root: impl Into<PathBuf>,
        action: ContainerAction,
    ) -> Self {
        Self {
            backend_id,
            policy_name: "default".to_owned(),
            repo_root: repo_root.into(),
            action,
            cleanup_result: None,
            state: ContainerRuntimeState::Unknown,
            notes: Vec::new(),
        }
    }

    pub fn policy_name(mut self, policy_name: impl Into<String>) -> Self {
        self.policy_name = policy_name.into();
        self
    }

    pub fn cleanup_result(mut self, cleanup_result: ContainerCleanupResult) -> Self {
        self.cleanup_result = Some(cleanup_result);
        self
    }

    pub fn state(mut self, state: ContainerRuntimeState) -> Self {
        self.state = state;
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerAction {
    Activate,
    Shell,
    Exec,
    Shutdown,
    Status,
    Stats,
    Logs,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerCleanupResult {
    NotRequested,
    Completed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerBackendCapabilities {
    pub can_attach: bool,
    pub can_repair_runtime: bool,
    pub can_copy: bool,
    pub can_stream_logs: bool,
}

pub trait ContainerBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn capabilities(&self) -> ContainerBackendCapabilities;

    fn compose_invocation(&self, repo_root: &Path) -> Vec<String>;

    fn status(&self, request: &ContainerManagerRequest) -> ContainerOperationReport {
        ContainerOperationReport::new(
            self.id(),
            request.repo_root.clone(),
            ContainerAction::Status,
        )
        .policy_name(interrupt_policy_name(request.interrupt_policy))
        .state(ContainerRuntimeState::Unknown)
    }
}

#[derive(Debug, Default)]
pub struct DockerComposeBackend;

impl ContainerBackend for DockerComposeBackend {
    fn id(&self) -> BackendId {
        BackendId::docker_compose()
    }

    fn capabilities(&self) -> ContainerBackendCapabilities {
        ContainerBackendCapabilities {
            can_attach: true,
            can_repair_runtime: false,
            can_copy: true,
            can_stream_logs: true,
        }
    }

    fn compose_invocation(&self, repo_root: &Path) -> Vec<String> {
        vec![
            "docker".to_owned(),
            "compose".to_owned(),
            "--project-directory".to_owned(),
            repo_root.display().to_string(),
        ]
    }
}

#[derive(Debug, Default)]
pub struct ColimaNerdctlBackend;

impl ContainerBackend for ColimaNerdctlBackend {
    fn id(&self) -> BackendId {
        BackendId::colima_nerdctl()
    }

    fn capabilities(&self) -> ContainerBackendCapabilities {
        ContainerBackendCapabilities {
            can_attach: true,
            can_repair_runtime: true,
            can_copy: true,
            can_stream_logs: true,
        }
    }

    fn compose_invocation(&self, repo_root: &Path) -> Vec<String> {
        vec![
            "nerdctl".to_owned(),
            "compose".to_owned(),
            "--project-directory".to_owned(),
            repo_root.display().to_string(),
        ]
    }
}

pub struct ContainerBackendRegistry {
    backends: Vec<Box<dyn ContainerBackend>>,
}

impl ContainerBackendRegistry {
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    pub fn defaults() -> Self {
        Self::new()
            .register(DockerComposeBackend)
            .register(ColimaNerdctlBackend)
    }

    pub fn register(mut self, backend: impl ContainerBackend + 'static) -> Self {
        self.backends.push(Box::new(backend));
        self
    }

    pub fn ids(&self) -> Vec<BackendId> {
        self.backends.iter().map(|backend| backend.id()).collect()
    }

    pub fn find(&self, id: &BackendId) -> Option<&dyn ContainerBackend> {
        self.backends
            .iter()
            .find(|backend| backend.id() == *id)
            .map(|backend| backend.as_ref())
    }

    pub fn select(
        &self,
        request: &ContainerManagerRequest,
    ) -> Result<&dyn ContainerBackend, ContainerManagerError> {
        if let Some(backend_id) = request.backend_override.as_ref() {
            return self
                .find(backend_id)
                .ok_or_else(|| ContainerManagerError::UnknownBackend {
                    backend_id: backend_id.clone(),
                });
        }

        self.backends
            .first()
            .map(|backend| backend.as_ref())
            .ok_or(ContainerManagerError::NoBackendsRegistered)
    }
}

impl Default for ContainerBackendRegistry {
    fn default() -> Self {
        Self::defaults()
    }
}

pub struct ContainerManager {
    registry: ContainerBackendRegistry,
}

impl ContainerManager {
    pub fn new(registry: ContainerBackendRegistry) -> Self {
        Self { registry }
    }

    pub fn defaults() -> Self {
        Self::new(ContainerBackendRegistry::defaults())
    }

    pub fn registry(&self) -> &ContainerBackendRegistry {
        &self.registry
    }

    pub fn selected_backend(
        &self,
        request: &ContainerManagerRequest,
    ) -> Result<&dyn ContainerBackend, ContainerManagerError> {
        self.registry.select(request)
    }

    pub fn status(
        &self,
        request: &ContainerManagerRequest,
    ) -> Result<ContainerOperationReport, ContainerManagerError> {
        Ok(self.selected_backend(request)?.status(request))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerManagerError {
    NoBackendsRegistered,
    UnknownBackend { backend_id: BackendId },
}

impl fmt::Display for ContainerManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBackendsRegistered => write!(f, "no container backends registered"),
            Self::UnknownBackend { backend_id } => {
                write!(f, "unknown container backend `{backend_id}`")
            }
        }
    }
}

impl std::error::Error for ContainerManagerError {}

fn interrupt_policy_name(policy: ContainerInterruptPolicy) -> &'static str {
    match policy {
        ContainerInterruptPolicy::Ignore => "ignore-interrupt",
        ContainerInterruptPolicy::Forward => "forward-interrupt",
        ContainerInterruptPolicy::ShutdownOnInterrupt => "shutdown-on-interrupt",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        BackendId, ColimaNerdctlBackend, ContainerAction, ContainerBackend,
        ContainerBackendRegistry, ContainerInterruptPolicy, ContainerManager,
        ContainerManagerError, ContainerManagerRequest, ContainerRuntimeState,
        DockerComposeBackend,
    };

    #[test]
    fn default_registry_exposes_docker_and_colima_backends() {
        let ids = ContainerBackendRegistry::defaults().ids();

        assert_eq!(
            ids,
            vec![BackendId::docker_compose(), BackendId::colima_nerdctl()]
        );
    }

    #[test]
    fn override_selects_colima_backend() {
        let request =
            ContainerManagerRequest::new("/tmp/repo").backend_override(BackendId::colima_nerdctl());
        let manager = ContainerManager::defaults();

        let backend = manager.selected_backend(&request).expect("backend");

        assert_eq!(backend.id(), BackendId::colima_nerdctl());
    }

    #[test]
    fn unknown_override_is_reported() {
        let request =
            ContainerManagerRequest::new("/tmp/repo").backend_override(BackendId::new("podman"));
        let manager = ContainerManager::defaults();

        let error = match manager.selected_backend(&request) {
            Ok(backend) => panic!("unexpected backend `{}`", backend.id()),
            Err(error) => error,
        };

        assert_eq!(
            error,
            ContainerManagerError::UnknownBackend {
                backend_id: BackendId::new("podman")
            }
        );
    }

    #[test]
    fn compose_invocations_are_stable() {
        let repo = PathBuf::from("/tmp/repo");

        assert_eq!(
            DockerComposeBackend.compose_invocation(&repo),
            vec!["docker", "compose", "--project-directory", "/tmp/repo"]
        );
        assert_eq!(
            ColimaNerdctlBackend.compose_invocation(&repo),
            vec!["nerdctl", "compose", "--project-directory", "/tmp/repo"]
        );
    }

    #[test]
    fn status_report_includes_required_identity_fields() {
        let request = ContainerManagerRequest::new("/tmp/repo")
            .backend_override(BackendId::colima_nerdctl())
            .interrupt_policy(ContainerInterruptPolicy::ShutdownOnInterrupt);
        let manager = ContainerManager::defaults();

        let report = manager.status(&request).expect("status report");

        assert_eq!(report.backend_id, BackendId::colima_nerdctl());
        assert_eq!(report.policy_name, "shutdown-on-interrupt");
        assert_eq!(report.repo_root, PathBuf::from("/tmp/repo"));
        assert_eq!(report.action, ContainerAction::Status);
        assert_eq!(report.cleanup_result, None);
        assert_eq!(report.state, ContainerRuntimeState::Unknown);
    }
}
