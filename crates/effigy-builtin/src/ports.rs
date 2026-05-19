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

/// Runtime services the built-in command layer depends on. Every
/// reach-back from the built-in layer into the rest of the runner goes
/// through this trait.
pub trait BuiltinRuntimePorts {
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
}
