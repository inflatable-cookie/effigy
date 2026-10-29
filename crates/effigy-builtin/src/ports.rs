//! Runtime ports the built-in command layer uses to reach back into the
//! runner.
//!
//! The runner's concrete `RunnerBuiltinPorts` implements this trait on
//! the task engine and translates `RunnerError` to `BuiltinError` at the
//! port boundary.

use std::any::Any;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_manifest::LoadedCatalog;
use serde::Serialize;

use crate::BuiltinError;

/// Exclusion scopes a built-in can hold for its call through the runner's
/// task engine. Each is a kernel gate, released when its holder exits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockScope {
    Workspace,
    Shared(String),
    Task(String),
    Profile { task: String, profile: String },
}

impl LockScope {
    pub fn label(&self) -> String {
        match self {
            Self::Workspace => "workspace".to_owned(),
            Self::Shared(name) => format!("shared:{name}"),
            Self::Task(task) => format!("task:{task}"),
            Self::Profile { task, profile } => format!("profile:{task}/{profile}"),
        }
    }
}

/// Opaque RAII handle returned by `acquire_scopes`. The built-in layer
/// holds it to keep the locks alive until the call scope ends and does
/// not introspect the inner value; the runner impl keeps its runs inside.
pub struct BuiltinLockGuards(#[allow(dead_code)] Box<dyn Any + Send>);

impl BuiltinLockGuards {
    pub fn new<T: Any + Send>(value: T) -> Self {
        Self(Box::new(value))
    }
}

impl std::fmt::Debug for BuiltinLockGuards {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltinLockGuards").finish_non_exhaustive()
    }
}

/// Process facts for one built-in suite command, delivered only when the
/// runtime port explicitly enables test diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct BuiltinTestChildEvidence {
    pub invocation_generation: u64,
    pub child_generation: u64,
    pub suite_name: String,
    pub root: String,
    pub pid: u32,
    pub parent_pid: u32,
    pub process_group: Option<i32>,
    pub process_group_error: Option<String>,
    pub observer_active_at_spawn: bool,
    pub start_observer_notified: bool,
    pub stop_observer_notified: bool,
    pub spawned: bool,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub wait_error: Option<String>,
}

/// One host suite's result under the caller's process custody.
#[derive(Debug, Clone, Default)]
pub struct HostSuiteOutcome {
    /// The suite command's exit code; `None` when a signal ended it.
    pub code: Option<i32>,
}

/// One host suite's process custody, owned by the caller's run. The suite runs
/// as a process group of that run, so the run's settle stops its tree before
/// the run's scratch directory is released, and the sentinel's settle does the
/// same when the owner dies.
pub trait HostSuiteCustody: Send {
    fn run(
        &mut self,
        command: &str,
        cwd: &Path,
        env: &std::collections::BTreeMap<String, String>,
        capture: bool,
    ) -> Result<HostSuiteOutcome, BuiltinError>;

    /// Records the suite's outcome and stops its process group.
    fn settle(self: Box<Self>, succeeded: bool);
}

/// Runtime services the built-in command layer depends on. Every
/// reach-back from the built-in layer into the rest of the runner goes
/// through this trait.
pub trait BuiltinRuntimePorts {
    /// The declared/effective backing-service aliases this checkout's host
    /// execution delivers (effigy#431, contract 005). A suite's environment,
    /// its rendered command and its `setup`/`teardown` commands receive the
    /// effective alias where they carry the declared one. Empty — the default,
    /// and every container-routed suite — leaves them as written: the
    /// container's own network answers both names.
    fn host_suite_env_aliases(
        &self,
        root: &std::path::Path,
    ) -> Result<Vec<crate::BuiltinHostAlias>, BuiltinError> {
        let _ = root;
        Ok(Vec::new())
    }

    /// Resolve the values this host suite would otherwise inherit unchanged
    /// from the caller's environment, with the scope's aliases and their
    /// publication proof (effigy#431 review). A key the suite's own environment
    /// delivers — its `env`, a step's `env`, or a rendered command's assignment
    /// — is never resolved or proved: that value already won.
    fn resolve_host_suite_inherited_env(
        &self,
        root: &std::path::Path,
        _label: &str,
        _delivered: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<(String, String)>, BuiltinError> {
        let _ = root;
        Ok(Vec::new())
    }

    /// Prove every delivered alias this suite would use before anything runs:
    /// the alias's route must belong to this checkout and publish an upstream
    /// port. `label` names the suite in the refusal. The default implementation
    /// refuses nothing because it delivers no alias.
    fn prove_host_suite_aliases(
        &self,
        root: &std::path::Path,
        label: &str,
        texts: &[&str],
    ) -> Result<(), BuiltinError> {
        let _ = (root, label, texts);
        Ok(())
    }

    /// Whether the caller wants test-only child evidence for built-in suite
    /// executions. Production implementations leave this disabled.
    fn builtin_test_child_evidence_enabled(&self) -> bool {
        false
    }

    /// Receive child evidence without influencing command execution. The
    /// default implementation intentionally discards it.
    fn record_builtin_test_child_evidence(&self, evidence: BuiltinTestChildEvidence) {
        let _ = evidence;
    }

    /// Host environment the caller's run gives the host processes a built-in
    /// starts: the per-run scratch directory's temp variables. Empty when the
    /// caller runs no such directory, and never forwarded into a container.
    fn host_scratch_env(&self) -> Vec<(String, std::ffi::OsString)> {
        Vec::new()
    }

    /// Whether the caller's run owns host suites, so the built-in must not put
    /// them behind a TUI that spawns its own processes.
    fn custody_holds_host_suites(&self) -> bool {
        false
    }

    fn has_test_tui_source(&self) -> bool {
        false
    }

    /// A view over the caller's process custody. The borrow ends before the
    /// caller settles its run, including when terminal initialization fails.
    fn test_tui_source(
        &self,
        specs: Vec<effigy_process::ProcessSpec>,
    ) -> Result<Option<Box<dyn effigy_tui::multiprocess::SessionProcesses + '_>>, BuiltinError>
    {
        let _ = specs;
        Ok(None)
    }

    /// The caller's run custody for one host suite. `None` keeps the built-in's
    /// own spawn, which is what a caller without a scratch run provides.
    fn host_suite_custody(
        &self,
        label: &str,
    ) -> Result<Option<Box<dyn HostSuiteCustody>>, BuiltinError> {
        let _ = label;
        Ok(None)
    }

    // Locking.
    fn acquire_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<BuiltinLockGuards, BuiltinError>;

    // Command context.
    fn current_working_dir(&self) -> Result<PathBuf, BuiltinError>;

    // Execute / nested command entry points.
    fn run_manifest_task_with_cwd(
        &self,
        task: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, BuiltinError>;

    fn run_doctor(&self, args: DoctorArgs) -> Result<String, BuiltinError>;

    fn run_tasks(&self, args: TasksArgs) -> Result<String, BuiltinError>;
    fn run_command(&self, command: Command) -> Result<String, BuiltinError>;

    // Deferred builtin introspection.
    fn deferred_builtins_from_catalogs(
        &self,
        catalogs: &[LoadedCatalog],
        resolved_root: &Path,
    ) -> BTreeSet<String>;

    // Container-routed test suite execution.
    //
    // Builtin test targets resolve their owning catalog's declared runtime
    // target during planning (`BuiltinTargetRuntime`). When that target is a
    // named container, these ports reach the runner's authoritative container
    // machinery: activation before execution and a faithful container exec
    // command line for each suite.

    /// Ensure the target's declared container is running and return the
    /// resolved suite target. `Ok(None)` means the runner treated the target
    /// as host-scoped (for example inside a container handoff).
    fn prepare_container_suite_target(
        &self,
        target_root: &Path,
        container: &str,
    ) -> Result<BuiltinContainerSuiteTarget, BuiltinError> {
        let _ = (target_root, container);
        Err(BuiltinError::task_invocation(
            "container suite execution is not available in this runtime",
        ))
    }

    /// Render one suite's lifecycle command as a container exec command line
    /// against the resolved suite target.
    fn render_container_suite_command(
        &self,
        target: &BuiltinContainerSuiteTarget,
        suite_command: &str,
    ) -> Result<String, BuiltinError> {
        let _ = target;
        Ok(suite_command.to_owned())
    }
}

/// A resolved, activated container suite target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinContainerSuiteTarget {
    pub container: String,
    pub service: String,
    /// The catalog root that owns the container policy (compose project).
    pub root: PathBuf,
}
