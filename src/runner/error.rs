use std::path::PathBuf;

use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_core::resolver::ResolveError;
use effigy_core::task_lock::TaskLockConflict;
use effigy_env::error::EnvSchemaError;
use effigy_managed::ManagedError;
use effigy_manifest::ManifestError;
use effigy_process::ProcessManagerError;
use effigy_routing::RoutingError;
use effigy_runtime::EffigyRuntimeError;
use effigy_scan::ScanError;
use effigy_tasks::TaskError;

#[path = "error/display.rs"]
mod display;
#[path = "error/rendered_output.rs"]
mod rendered_output;

#[derive(Debug)]
pub enum RunnerError {
    Cwd(std::io::Error),
    Resolve(ResolveError),
    Task(TaskError),
    Ui(String),
    TaskInvocation(String),
    ContainerSurfaceRegistryMissing,
    ContainerSurfaceDefaultTargetMissing,
    ContainerSurfaceNotDefined {
        container: String,
    },
    ContainerSurfaceNotRunning {
        container: String,
    },
    ContainerSurfacePolicy {
        phase: &'static str,
        container: String,
        detail: String,
    },
    WorkspaceSessionCleanup {
        shell_error: String,
        cleanup_error: String,
    },
    HostContainerLeaseEncode {
        detail: String,
    },
    HostContainerLeaseReaperBootstrap {
        detail: String,
    },
    GatewayRouteTable {
        phase: &'static str,
        path: PathBuf,
        detail: String,
    },
    GatewayRouteRegistration {
        phase: &'static str,
        domain: String,
        detail: String,
    },
    GatewayRouteShape {
        phase: &'static str,
        detail: String,
    },
    GatewayLoopback {
        phase: &'static str,
        detail: String,
    },
    GatewayRuntimeTarget {
        phase: &'static str,
        detail: String,
    },
    ContainerRuntimePolicy {
        phase: &'static str,
        detail: String,
    },
    ContainerRuntimeExecNotReady {
        container: String,
        service: String,
        profile: String,
        working_dir: PathBuf,
    },
    TaskCatalogsMissing {
        root: PathBuf,
    },
    TaskManifestRead {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskManifestParse {
        path: PathBuf,
        error: toml::de::Error,
    },
    TaskManifestCompose {
        path: PathBuf,
        detail: String,
    },
    TaskCatalogAliasConflict {
        alias: String,
        first_path: PathBuf,
        second_path: PathBuf,
    },
    TaskCatalogPrefixNotFound {
        prefix: String,
        available: Vec<String>,
    },
    TaskNotFound {
        name: String,
        path: PathBuf,
    },
    TaskNotFoundAny {
        name: String,
        catalogs: Vec<String>,
    },
    TaskAmbiguous {
        name: String,
        candidates: Vec<String>,
    },
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
    TaskCommandFailure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    TaskLockConflict(Box<TaskLockConflict>),
    TaskLockIo {
        path: PathBuf,
        error: std::io::Error,
    },
    CommandJsonFailure {
        rendered: String,
    },
    DepsOperationNonZero {
        command: &'static str,
        outcome: &'static str,
        error_count: usize,
        rendered: String,
    },
    ManagedProcess(ProcessManagerError),
    TaskManagedUnsupportedMode {
        task: String,
        mode: String,
    },
    TaskManagedProfileNotFound {
        task: String,
        profile: String,
        available: Vec<String>,
    },
    TaskManagedProfileEmpty {
        task: String,
        profile: String,
    },
    TaskManagedProcessNotFound {
        task: String,
        profile: String,
        process: String,
    },
    TaskManagedProcessInvalidDefinition {
        task: String,
        process: String,
        detail: String,
    },
    TaskManagedProfileTabOrderInvalid {
        task: String,
        profile: String,
        detail: String,
    },
    TaskManagedTaskReferenceInvalid {
        task: String,
        process: String,
        reference: String,
        detail: String,
    },
    TaskManagedNonZeroExit {
        task: String,
        profile: String,
        processes: Vec<(String, String)>,
    },
    TaskMissingRunCommand {
        task: String,
        path: PathBuf,
    },
    BuiltinTestNonZero {
        failures: Vec<(String, Option<i32>)>,
        rendered: String,
    },
    BuiltinScanNonZero {
        finding_count: usize,
        rendered: String,
    },
    DoctorNonZero {
        error_count: usize,
        rendered: String,
    },
    DeferLoopDetected {
        depth: u8,
    },
    EnvSchema(EnvSchemaError),
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display::fmt_runner_error(self, f)
    }
}

impl std::error::Error for RunnerError {}

impl RunnerError {
    pub fn rendered_output(&self) -> Option<&str> {
        rendered_output::runner_error_rendered_output(self)
    }

    pub(in crate::runner) fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }

    pub(in crate::runner) fn container_runtime_policy(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::ContainerRuntimePolicy {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn container_runtime_exec_not_ready(
        policy: &effigy_containers::EffectiveContainerPolicy,
        working_dir: &std::path::Path,
    ) -> Self {
        Self::ContainerRuntimeExecNotReady {
            container: policy.name.clone(),
            service: policy.primary_service.clone(),
            profile: policy.profile.clone(),
            working_dir: working_dir.to_path_buf(),
        }
    }

    pub(in crate::runner) fn container_surface_not_defined(container: impl Into<String>) -> Self {
        Self::ContainerSurfaceNotDefined {
            container: container.into(),
        }
    }

    pub(in crate::runner) fn container_surface_not_running(container: impl Into<String>) -> Self {
        Self::ContainerSurfaceNotRunning {
            container: container.into(),
        }
    }

    pub(in crate::runner) fn container_surface_policy(
        phase: &'static str,
        container: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::ContainerSurfacePolicy {
            phase,
            container: container.into(),
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn workspace_session_cleanup(
        shell_error: impl Into<String>,
        cleanup_error: impl Into<String>,
    ) -> Self {
        Self::WorkspaceSessionCleanup {
            shell_error: shell_error.into(),
            cleanup_error: cleanup_error.into(),
        }
    }

    pub(in crate::runner) fn host_container_lease_encode(detail: impl Into<String>) -> Self {
        Self::HostContainerLeaseEncode {
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn host_container_lease_reaper_bootstrap(
        detail: impl Into<String>,
    ) -> Self {
        Self::HostContainerLeaseReaperBootstrap {
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_route_table(
        phase: &'static str,
        path: impl Into<PathBuf>,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRouteTable {
            phase,
            path: path.into(),
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_route_registration(
        phase: &'static str,
        domain: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRouteRegistration {
            phase,
            domain: domain.into(),
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_route_shape(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRouteShape {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_loopback(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayLoopback {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn gateway_runtime_target(
        phase: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self::GatewayRuntimeTarget {
            phase,
            detail: detail.into(),
        }
    }

    pub(in crate::runner) fn task_invocation_failed_read(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_read_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_parse(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_parse_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_write(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_write_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_render(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_render_path(path, error))
    }
}

impl From<TaskError> for RunnerError {
    fn from(value: TaskError) -> Self {
        Self::Task(value)
    }
}

impl From<effigy_ui::UiError> for RunnerError {
    fn from(value: effigy_ui::UiError) -> Self {
        Self::Ui(value.to_string())
    }
}

impl From<ResolveError> for RunnerError {
    fn from(value: ResolveError) -> Self {
        Self::Resolve(value)
    }
}

impl From<ProcessManagerError> for RunnerError {
    fn from(value: ProcessManagerError) -> Self {
        Self::ManagedProcess(value)
    }
}

impl From<EnvSchemaError> for RunnerError {
    fn from(value: EnvSchemaError) -> Self {
        Self::EnvSchema(value)
    }
}

impl From<EffigyRuntimeError> for RunnerError {
    fn from(value: EffigyRuntimeError) -> Self {
        match value {
            EffigyRuntimeError::Cwd(error) => Self::Cwd(error),
            EffigyRuntimeError::Ui(message) => Self::Ui(message),
            EffigyRuntimeError::TaskInvocation(message) => Self::TaskInvocation(message),
            EffigyRuntimeError::TaskCommandLaunch { command, error } => {
                Self::TaskCommandLaunch { command, error }
            }
        }
    }
}

impl From<ManagedError> for RunnerError {
    fn from(value: ManagedError) -> Self {
        match value {
            ManagedError::Cwd(error) => Self::Cwd(error),
            ManagedError::TaskInvocation(message) => Self::TaskInvocation(message),
            ManagedError::Ui(message) => Self::Ui(message),
            ManagedError::Process(error) => Self::ManagedProcess(error),
            ManagedError::TaskManagedUnsupportedMode { task, mode } => {
                Self::TaskManagedUnsupportedMode { task, mode }
            }
            ManagedError::TaskHasConcurrentWithoutMode { task } => Self::TaskInvocation(format!(
                "task '{task}' declares `concurrent = [...]` but does not set `mode = \"tui\"`; \
                     either add `mode = \"tui\"` (TUI runs concurrent entries as tabs) or move \
                     the concurrent entries to `[[containers.<name>.host_processes]]` for non-TUI \
                     lifecycle-bound supervisors"
            )),
            ManagedError::TaskManagedProfileNotFound {
                task,
                profile,
                available,
            } => Self::TaskManagedProfileNotFound {
                task,
                profile,
                available,
            },
            ManagedError::TaskManagedProfileEmpty { task, profile } => {
                Self::TaskManagedProfileEmpty { task, profile }
            }
            ManagedError::TaskManagedProcessInvalidDefinition {
                task,
                process,
                detail,
            } => Self::TaskManagedProcessInvalidDefinition {
                task,
                process,
                detail,
            },
            ManagedError::TaskManagedTaskReferenceInvalid {
                task,
                process,
                reference,
                detail,
            } => Self::TaskManagedTaskReferenceInvalid {
                task,
                process,
                reference,
                detail,
            },
            ManagedError::TaskManagedNonZeroExit {
                task,
                profile,
                processes,
            } => Self::TaskManagedNonZeroExit {
                task,
                profile,
                processes,
            },
        }
    }
}

impl From<effigy_demo::DemoStateError> for RunnerError {
    fn from(value: effigy_demo::DemoStateError) -> Self {
        Self::task_invocation(value.to_string())
    }
}

impl From<effigy_distribution::DistributionPolicyError> for RunnerError {
    fn from(value: effigy_distribution::DistributionPolicyError) -> Self {
        match value {
            effigy_distribution::DistributionPolicyError::Manifest(error) => {
                Self::task_invocation(error.to_string())
            }
        }
    }
}

impl From<effigy_distribution::DistributionExecutionError> for RunnerError {
    fn from(value: effigy_distribution::DistributionExecutionError) -> Self {
        match value {
            effigy_distribution::DistributionExecutionError::Io { path, error } => {
                Self::task_invocation_failed_write(&path, error)
            }
            effigy_distribution::DistributionExecutionError::Message(message) => {
                Self::task_invocation(message)
            }
        }
    }
}

impl From<effigy_containers::ContainerPolicyError> for RunnerError {
    fn from(value: effigy_containers::ContainerPolicyError) -> Self {
        match value {
            effigy_containers::ContainerPolicyError::Manifest(error) => {
                Self::task_invocation(error.to_string())
            }
            effigy_containers::ContainerPolicyError::Catalog(error) => {
                Self::task_invocation(error.to_string())
            }
            effigy_containers::ContainerPolicyError::TaskInvocation(message) => {
                Self::task_invocation(message)
            }
            effigy_containers::ContainerPolicyError::Read { path, error } => {
                Self::task_invocation_failed_read(&path, error)
            }
        }
    }
}

impl From<effigy_release::ReleaseError> for RunnerError {
    fn from(value: effigy_release::ReleaseError) -> Self {
        Self::task_invocation(value.to_string())
    }
}

/// Lifts `RoutingError` from the `catalog/**` routing surface into the
/// existing `RunnerError::Task*` variants. Shapes are kept identical so
/// this is a mechanical variant-to-variant map; the `Manifest` wrapper
/// variant bridges catalog's own load path (card `245` Part B) by
/// delegating through the existing `ManifestError` → `RunnerError`
/// mapping. Card `246` will move this impl to a shim once `catalog/**`
/// extracts into `effigy-routing`.
impl From<RoutingError> for RunnerError {
    fn from(value: RoutingError) -> Self {
        match value {
            RoutingError::TaskCatalogsMissing { root } => Self::TaskCatalogsMissing { root },
            RoutingError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path,
            } => Self::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path,
            },
            RoutingError::TaskCatalogMemberInvalid { .. } => {
                Self::TaskInvocation(value.to_string())
            }
            RoutingError::TaskCatalogPrefixNotFound { prefix, available } => {
                Self::TaskCatalogPrefixNotFound { prefix, available }
            }
            RoutingError::TaskNotFound { name, path } => Self::TaskNotFound { name, path },
            RoutingError::TaskNotFoundAny { name, catalogs } => {
                Self::TaskNotFoundAny { name, catalogs }
            }
            RoutingError::TaskAmbiguous { name, candidates } => {
                Self::TaskAmbiguous { name, candidates }
            }
            RoutingError::Manifest(error) => map_manifest_error(error),
        }
    }
}

/// Lifts `ScanError` from `effigy-scan` into `RunnerError` at the
/// runner's edge. Job-8 pattern — see also `effigy-process`,
/// `effigy-ui`, `effigy-managed`, `effigy-env`, `effigy-routing`.
/// The `Invocation` variant covers the 20 call sites that previously
/// constructed `RunnerError::task_invocation(...)` directly. The
/// `Manifest` variant bridges scan's option-loading path through the
/// existing `ManifestError` → `RunnerError` mapping.
impl From<ScanError> for RunnerError {
    fn from(value: ScanError) -> Self {
        match value {
            ScanError::Invocation(message) => Self::TaskInvocation(message),
            ScanError::Manifest(error) => map_manifest_error(error),
        }
    }
}

fn map_manifest_error(error: ManifestError) -> RunnerError {
    match error {
        ManifestError::Read { path, error } => RunnerError::TaskManifestRead { path, error },
        ManifestError::Parse { path, error } => RunnerError::TaskManifestParse { path, error },
        ManifestError::Compose { path, detail } => {
            RunnerError::TaskManifestCompose { path, detail }
        }
        ManifestError::Render { path, detail } => {
            RunnerError::task_invocation_failed_render(&path, detail)
        }
    }
}

/// Lifts `BuiltinError` from `effigy-builtin` into `RunnerError` at
/// the runner's edge. Variant shapes mirror one-for-one (card 250),
/// with the `Manifest` variant bridging through
/// `map_manifest_error` to match the pattern used by `ScanError` and
/// `RoutingError`.
impl From<effigy_builtin::BuiltinError> for RunnerError {
    fn from(value: effigy_builtin::BuiltinError) -> Self {
        use effigy_builtin::BuiltinError as B;
        match value {
            B::TaskInvocation(message) => Self::TaskInvocation(message),
            B::Ui(message) => Self::Ui(message),
            B::TaskManifestCompose { path, detail } => Self::TaskManifestCompose { path, detail },
            B::TaskCommandLaunch { command, error } => Self::TaskCommandLaunch { command, error },
            B::TaskLockConflict(details) => Self::TaskLockConflict(details),
            B::TaskLockIo { path, error } => Self::TaskLockIo { path, error },
            B::BuiltinTestNonZero { failures, rendered } => {
                Self::BuiltinTestNonZero { failures, rendered }
            }
            B::BuiltinScanNonZero {
                finding_count,
                rendered,
            } => Self::BuiltinScanNonZero {
                finding_count,
                rendered,
            },
            B::DoctorNonZero {
                error_count,
                rendered,
            } => Self::DoctorNonZero {
                error_count,
                rendered,
            },
            B::Manifest(error) => map_manifest_error(error),
            B::Managed(error) => Self::from(error),
            B::Routing(error) => Self::from(error),
            B::Scan(error) => Self::from(error),
        }
    }
}

/// Lifts `DoctorError` from `effigy-doctor` into `RunnerError` at the
/// runner's edge (card 254). Mirrors the `From<BuiltinError>` pattern.
impl From<effigy_doctor::DoctorError> for RunnerError {
    fn from(value: effigy_doctor::DoctorError) -> Self {
        use effigy_doctor::DoctorError as D;
        match value {
            D::DoctorNonZero {
                error_count,
                rendered,
            } => Self::DoctorNonZero {
                error_count,
                rendered,
            },
            D::TaskInvocation(message) => Self::TaskInvocation(message),
            D::Ui(message) => Self::Ui(message),
            D::CommandJsonFailure { rendered } => Self::CommandJsonFailure { rendered },
            D::Manifest(error) => map_manifest_error(error),
            D::Scan(error) => Self::from(error),
            D::Routing(error) => Self::from(error),
        }
    }
}

impl From<effigy_containers::exec::ContainerExecError> for RunnerError {
    fn from(value: effigy_containers::exec::ContainerExecError) -> Self {
        match value {
            effigy_containers::exec::ContainerExecError::Launch { command, error } => {
                Self::TaskCommandLaunch { command, error }
            }
            effigy_containers::exec::ContainerExecError::Failure {
                command,
                code,
                stdout,
                stderr,
            } => map_container_exec_failure(command, code, stdout, stderr),
        }
    }
}

fn map_container_exec_failure(
    command: String,
    code: Option<i32>,
    stdout: String,
    stderr: String,
) -> RunnerError {
    let lowered = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if lowered.contains("error retrieving current runtime: empty value") {
        return RunnerError::TaskInvocation(format!(
            "Colima profile runtime state is corrupted and Effigy could not recover it automatically.\nprofile command: `{command}`\nnext: restart or delete the affected Colima profile, then retry the Effigy command.\ndetails:\n{stderr}"
        ));
    }
    if lowered.contains("no space left on device") {
        return RunnerError::TaskInvocation(format!(
            "Container runtime storage is out of space or inodes while running `{command}`.\nnext: run `effigy container profile resize` to restart the managed Colima profile without deleting runtime data, then retry the Effigy command. If disk pressure remains, inspect `effigy container cache list --global` and prune purge-safe caches with `effigy container cache prune --global --yes`.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        ));
    }

    RunnerError::TaskCommandFailure {
        command,
        code,
        stdout,
        stderr,
    }
}

#[cfg(test)]
#[path = "error/tests.rs"]
mod tests;
