use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ExecutionSurface;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusTargetIdentity {
    pub repo_root: PathBuf,
    pub selected_catalog_root: PathBuf,
    pub resolved_selector: String,
    pub resolved_task_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_profile: Option<String>,
}

impl TaskStatusTargetIdentity {
    pub fn new(
        repo_root: PathBuf,
        selected_catalog_root: PathBuf,
        resolved_selector: impl Into<String>,
        resolved_task_name: impl Into<String>,
        resolved_profile: Option<String>,
    ) -> Self {
        Self {
            repo_root,
            selected_catalog_root,
            resolved_selector: resolved_selector.into(),
            resolved_task_name: resolved_task_name.into(),
            resolved_profile,
        }
    }

    pub fn status_key(&self) -> TaskStatusKey {
        TaskStatusKey::from_identity(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskStatusKey(String);

impl TaskStatusKey {
    pub fn from_identity(identity: &TaskStatusTargetIdentity) -> Self {
        let selector_slug = safe_slug(&identity.resolved_selector);
        let catalog_slug = identity
            .selected_catalog_root
            .file_name()
            .and_then(|name| name.to_str())
            .map(safe_slug)
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| "task".to_owned());
        let digest = fnv1a64(&[
            &normalize_path(&identity.repo_root),
            &normalize_path(&identity.selected_catalog_root),
            &identity.resolved_selector,
            identity.resolved_profile.as_deref().unwrap_or(""),
        ]);
        Self(format!("{catalog_slug}-{selector_slug}-{digest:016x}"))
    }

    pub fn from_storage_name(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TaskStatusKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatusState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatusStage {
    Routing,
    WaitingForLock,
    RuntimePrep,
    Executing,
    ManagedSession,
    Handoff,
    Finishing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusRuntimeRouteSummary {
    pub route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusOutcome {
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusActiveRecord {
    pub status_key: TaskStatusKey,
    pub identity: TaskStatusTargetIdentity,
    pub state: TaskStatusState,
    pub stage: TaskStatusStage,
    pub execution_surface: ExecutionSurface,
    pub runtime_route: TaskStatusRuntimeRouteSummary,
    pub owner_pid: u32,
    pub started_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub lock_scopes: Vec<String>,
    pub active_record_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStatusCompletedRecord {
    pub status_key: TaskStatusKey,
    pub identity: TaskStatusTargetIdentity,
    pub state: TaskStatusState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<TaskStatusStage>,
    pub execution_surface: ExecutionSurface,
    pub runtime_route: TaskStatusRuntimeRouteSummary,
    pub started_at: String,
    pub finished_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub lock_scopes: Vec<String>,
    pub outcome: TaskStatusOutcome,
    pub latest_report_path: String,
    pub history_report_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_kind", rename_all = "kebab-case")]
pub enum TaskStatusRecord {
    Active(TaskStatusActiveRecord),
    Completed(TaskStatusCompletedRecord),
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn safe_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        let mapped = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            _ => None,
        };
        if let Some(ch) = mapped {
            slug.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let truncated = slug.chars().take(48).collect::<String>();
    if truncated.is_empty() {
        "task".to_owned()
    } else {
        truncated
    }
}

fn fnv1a64(parts: &[&str]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{TaskStatusKey, TaskStatusTargetIdentity};

    fn identity(catalog_root: &str) -> TaskStatusTargetIdentity {
        TaskStatusTargetIdentity::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from(catalog_root),
            "api/test",
            "test",
            None,
        )
    }

    #[test]
    fn task_status_key_is_deterministic_for_same_identity() {
        let identity = identity("/tmp/repo/api");
        let one = TaskStatusKey::from_identity(&identity);
        let two = TaskStatusKey::from_identity(&identity);
        assert_eq!(one, two);
        assert!(one.as_str().starts_with("api-api-test-"));
    }

    #[test]
    fn task_status_key_differs_for_descendant_scope_collisions() {
        let root_task = identity("/tmp/repo/api");
        let descendant_task = identity("/tmp/repo/services/api");
        assert_ne!(
            TaskStatusKey::from_identity(&root_task),
            TaskStatusKey::from_identity(&descendant_task)
        );
    }

    #[test]
    fn task_status_key_stays_filesystem_safe() {
        let identity = TaskStatusTargetIdentity::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from("/tmp/repo/api"),
            "api/test:weird value",
            "test",
            Some("uat profile".to_owned()),
        );
        let key = TaskStatusKey::from_identity(&identity);
        assert!(key
            .as_str()
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'));
    }
}
