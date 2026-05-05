use std::ffi::OsString;
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
pub struct ContainerBackendDetection {
    pub backend_override: Option<BackendId>,
    pub docker_cli_available: bool,
}

impl ContainerBackendDetection {
    pub fn new(docker_cli_available: bool) -> Self {
        Self {
            backend_override: None,
            docker_cli_available,
        }
    }

    pub fn backend_override(mut self, backend: BackendId) -> Self {
        self.backend_override = Some(backend);
        self
    }

    pub fn from_env_and_path() -> Self {
        Self {
            backend_override: backend_override_from_env(),
            docker_cli_available: command_exists("docker"),
        }
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
    fn compose_process_invocation(
        &self,
        profile: &str,
        args: &[OsString],
    ) -> (OsString, Vec<OsString>);

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

    fn compose_process_invocation(
        &self,
        _profile: &str,
        args: &[OsString],
    ) -> (OsString, Vec<OsString>) {
        (OsString::from("docker"), args.to_vec())
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

    fn compose_process_invocation(
        &self,
        profile: &str,
        args: &[OsString],
    ) -> (OsString, Vec<OsString>) {
        let mut resolved = vec![
            OsString::from("nerdctl"),
            OsString::from("--profile"),
            OsString::from(profile),
            OsString::from("--"),
        ];
        resolved.extend(args.iter().cloned());
        (OsString::from("colima"), resolved)
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

    pub fn detect_backend(
        &self,
        detection: &ContainerBackendDetection,
    ) -> Result<BackendId, ContainerManagerError> {
        let backend_id = if let Some(backend_id) = detection.backend_override.as_ref() {
            backend_id.clone()
        } else if detection.docker_cli_available {
            BackendId::docker_compose()
        } else {
            BackendId::colima_nerdctl()
        };

        self.find(&backend_id)
            .map(|backend| backend.id())
            .ok_or(ContainerManagerError::UnknownBackend { backend_id })
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

    pub fn compose_process_invocation(
        &self,
        detection: &ContainerBackendDetection,
        profile: &str,
        args: &[OsString],
    ) -> Result<(OsString, Vec<OsString>), ContainerManagerError> {
        let backend_id = self.registry.detect_backend(detection)?;
        let backend =
            self.registry
                .find(&backend_id)
                .ok_or(ContainerManagerError::UnknownBackend {
                    backend_id: backend_id.clone(),
                })?;
        Ok(backend.compose_process_invocation(profile, args))
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

pub fn backend_override_from_env_value(value: &str) -> Option<BackendId> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => None,
        "docker" | "docker-compose" => Some(BackendId::docker_compose()),
        "colima" | "colima-nerdctl" | "nerdctl" | "containerd" => Some(BackendId::colima_nerdctl()),
        _ => None,
    }
}

pub fn resolve_host_cli_program(program: &str) -> OsString {
    resolve_host_cli_program_path(program)
        .map(|path| path.into_os_string())
        .unwrap_or_else(|| OsString::from(program))
}

fn backend_override_from_env() -> Option<BackendId> {
    std::env::var("EFFIGY_COMPOSE_BACKEND")
        .ok()
        .and_then(|value| backend_override_from_env_value(&value))
}

fn command_exists(program: &str) -> bool {
    resolve_host_cli_program_path(program).is_some()
}

fn resolve_host_cli_program_path(program: &str) -> Option<PathBuf> {
    resolve_host_cli_program_path_with_extra(program, &[])
}

fn resolve_host_cli_program_path_with_extra(
    program: &str,
    extra_dirs: &[PathBuf],
) -> Option<PathBuf> {
    if program.contains(std::path::MAIN_SEPARATOR) {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }

    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path));
    }
    candidates.extend(extra_dirs.iter().cloned());
    #[cfg(target_os = "macos")]
    {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/Applications/Docker.app/Contents/Resources/bin"),
            PathBuf::from("/Applications/OrbStack.app/Contents/MacOS/bin"),
        ]);
    }

    candidates
        .into_iter()
        .map(|entry| entry.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{
        backend_override_from_env_value, BackendId, ColimaNerdctlBackend, ContainerAction,
        ContainerBackend, ContainerBackendDetection, ContainerBackendRegistry,
        ContainerInterruptPolicy, ContainerManager, ContainerManagerError, ContainerManagerRequest,
        ContainerRuntimeState, DockerComposeBackend,
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
    fn detection_prefers_explicit_override() {
        let registry = ContainerBackendRegistry::defaults();
        let detection =
            ContainerBackendDetection::new(true).backend_override(BackendId::colima_nerdctl());

        let backend_id = registry.detect_backend(&detection).expect("backend id");

        assert_eq!(backend_id, BackendId::colima_nerdctl());
    }

    #[test]
    fn detection_prefers_docker_when_available() {
        let registry = ContainerBackendRegistry::defaults();

        let backend_id = registry
            .detect_backend(&ContainerBackendDetection::new(true))
            .expect("backend id");

        assert_eq!(backend_id, BackendId::docker_compose());
    }

    #[test]
    fn detection_falls_back_to_colima_when_docker_is_unavailable() {
        let registry = ContainerBackendRegistry::defaults();

        let backend_id = registry
            .detect_backend(&ContainerBackendDetection::new(false))
            .expect("backend id");

        assert_eq!(backend_id, BackendId::colima_nerdctl());
    }

    #[test]
    fn backend_override_values_match_legacy_compose_env() {
        assert_eq!(
            backend_override_from_env_value("docker"),
            Some(BackendId::docker_compose())
        );
        assert_eq!(
            backend_override_from_env_value("containerd"),
            Some(BackendId::colima_nerdctl())
        );
        assert_eq!(backend_override_from_env_value("auto"), None);
        assert_eq!(backend_override_from_env_value("unknown"), None);
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
    fn compose_process_invocations_are_stable() {
        let args = vec![OsString::from("compose"), OsString::from("ps")];

        assert_eq!(
            DockerComposeBackend.compose_process_invocation("effigy", &args),
            (OsString::from("docker"), args.clone())
        );
        assert_eq!(
            ColimaNerdctlBackend.compose_process_invocation("effigy", &args),
            (
                OsString::from("colima"),
                vec![
                    OsString::from("nerdctl"),
                    OsString::from("--profile"),
                    OsString::from("effigy"),
                    OsString::from("--"),
                    OsString::from("compose"),
                    OsString::from("ps"),
                ]
            )
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
