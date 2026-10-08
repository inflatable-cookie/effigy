//! Runtime ports the built-in command layer uses to reach back into the
//! runner.
//!
//! Card 251 inverted `super::super::super::{locking,cache,...}`
//! reach-ins behind this trait. Card 250 moved the trait + the
//! adjacent data types (`LockScope`, `UnlockResult`, `TaskCacheEntry`,
//! `BuiltinLockGuards`) into the `effigy-builtin` crate. The runner's
//! concrete `RunnerBuiltinPorts` impl lives alongside the runner's
//! locking / cache / execute modules and translates `RunnerError` to
//! `BuiltinError` at the port boundary.

use std::any::Any;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use effigy_cli::{Command, DoctorArgs, TaskInvocation, TasksArgs};
use effigy_manifest::LoadedCatalog;
use serde::{Deserialize, Serialize};

use crate::BuiltinError;

/// Lock scopes the runner can acquire / release. Moved from
/// `runner::locking::model` under card 250 so the port trait can sit in
/// `effigy-builtin` without pulling runner-internal types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockScope {
    Workspace,
    Shared(String),
    Task(String),
    Profile { task: String, profile: String },
}

impl LockScope {
    pub fn parse(value: &str) -> Option<Self> {
        let raw = value.trim();
        if raw == "workspace" {
            return Some(Self::Workspace);
        }
        if let Some(name) = raw.strip_prefix("shared:") {
            let name = name.trim();
            if !name.is_empty() {
                return Some(Self::Shared(name.to_owned()));
            }
            return None;
        }
        if let Some(task) = raw.strip_prefix("task:") {
            let task = task.trim();
            if !task.is_empty() {
                return Some(Self::Task(task.to_owned()));
            }
            return None;
        }
        if let Some(rest) = raw.strip_prefix("profile:") {
            let rest = rest.trim();
            let (task, profile) = rest.split_once('/')?;
            let task = task.trim();
            let profile = profile.trim();
            if task.is_empty() || profile.is_empty() {
                return None;
            }
            return Some(Self::Profile {
                task: task.to_owned(),
                profile: profile.to_owned(),
            });
        }
        None
    }

    /// Parse an unlock target. Typed scopes keep their current meaning.
    /// A task selector such as `validate:activity-routing` maps to
    /// `task:validate:activity-routing` instead of silently targeting a
    /// different lock family.
    pub fn parse_unlock_target(value: &str) -> Result<Self, String> {
        let raw = value.trim();
        if let Some(scope) = Self::parse(raw) {
            return Ok(scope);
        }
        if incomplete_typed_unlock_scope(raw)
            || raw.is_empty()
            || raw.chars().any(char::is_whitespace)
        {
            return Err(format!(
                "unlock target `{raw}` is invalid; expected `workspace`, `shared:<name>`, `task:<name>`, `profile:<task>/<profile>`, or a task selector such as `validate:activity-routing` (unlocks `task:validate:activity-routing`)"
            ));
        }
        Ok(Self::Task(raw.to_owned()))
    }

    pub fn label(&self) -> String {
        match self {
            Self::Workspace => "workspace".to_owned(),
            Self::Shared(name) => format!("shared:{name}"),
            Self::Task(task) => format!("task:{task}"),
            Self::Profile { task, profile } => format!("profile:{task}/{profile}"),
        }
    }

    pub fn file_name(&self) -> String {
        format!("{}.lock", sanitize_for_file_name(&self.label()))
    }
}

fn sanitize_for_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect::<String>()
}

fn incomplete_typed_unlock_scope(raw: &str) -> bool {
    raw == "shared"
        || raw
            .strip_prefix("shared:")
            .is_some_and(|name| name.trim().is_empty())
        || raw
            .strip_prefix("task:")
            .is_some_and(|name| name.trim().is_empty())
        || raw == "profile"
        || raw.strip_prefix("profile:").is_some_and(|rest| {
            rest.split_once('/')
                .map(|(task, profile)| task.trim().is_empty() || profile.trim().is_empty())
                .unwrap_or(true)
        })
}

/// Outcome of releasing one or more lock scopes. Moved from
/// `runner::locking::io` under card 250.
pub struct UnlockResult {
    pub removed: Vec<String>,
    pub missing: Vec<String>,
}

/// Opaque RAII handle returned by `acquire_scopes`. The built-in layer
/// holds it to keep the locks alive until the call scope ends and does
/// not introspect the inner value; the runner impl stuffs its
/// `Vec<LockGuard>` inside.
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

/// Cache entry surfaced through `cache_entries` / `cache_entry` ports.
/// Moved from `runner::cache::model` under card 250.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCacheEntry {
    pub key: String,
    pub task_name: String,
    pub manifest_path: String,
    pub catalog_root: String,
    pub fingerprint: String,
    pub command: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub env_keys: Vec<String>,
    pub outputs_exist: bool,
    pub updated_at_epoch_ms: u128,
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

/// Runtime services the built-in command layer depends on. Every
/// reach-back from the built-in layer into the rest of the runner goes
/// through this trait.
pub trait BuiltinRuntimePorts {
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

    // Locking.
    fn acquire_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<BuiltinLockGuards, BuiltinError>;

    fn unlock_scopes(
        &self,
        workspace_root: &Path,
        scopes: &[LockScope],
    ) -> Result<UnlockResult, BuiltinError>;

    fn unlock_all(&self, workspace_root: &Path) -> Result<UnlockResult, BuiltinError>;

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

    // Cache inspection / invalidation.
    fn cache_entries(&self, workspace_root: &Path) -> Result<Vec<TaskCacheEntry>, BuiltinError>;

    fn cache_entry(
        &self,
        workspace_root: &Path,
        manifest_path: &Path,
        task_name: &str,
    ) -> Result<Option<TaskCacheEntry>, BuiltinError>;

    fn cache_entry_key(&self, manifest_path: &Path, task_name: &str) -> String;

    fn invalidate_cache_keys(
        &self,
        workspace_root: &Path,
        keys: &[String],
    ) -> Result<Vec<String>, BuiltinError>;

    fn invalidate_all_cache_entries(&self, workspace_root: &Path) -> Result<usize, BuiltinError>;

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

#[cfg(test)]
mod tests {
    use super::LockScope;

    #[test]
    fn parse_unlock_target_maps_task_selectors_to_task_scopes() {
        assert_eq!(
            LockScope::parse_unlock_target("validate:activity-routing").expect("selector"),
            LockScope::Task("validate:activity-routing".to_owned())
        );
        assert_eq!(
            LockScope::parse_unlock_target("dev").expect("selector"),
            LockScope::Task("dev".to_owned())
        );
        assert_eq!(
            LockScope::parse_unlock_target("task:dev").expect("typed"),
            LockScope::Task("dev".to_owned())
        );
    }

    #[test]
    fn parse_unlock_target_rejects_incomplete_typed_scopes_with_selector_example() {
        let err = LockScope::parse_unlock_target("profile:foo").expect_err("incomplete");
        assert!(err.contains("profile:<task>/<profile>"));
        assert!(err.contains("task:validate:activity-routing"));
    }
}
