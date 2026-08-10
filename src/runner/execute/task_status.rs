use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use chrono::Utc;
use effigy_execution::{
    ExecutionSelectionPlan, ExecutionSurface, TaskStatusActiveRecord, TaskStatusCompletedRecord,
    TaskStatusKey, TaskStatusOutcome, TaskStatusRuntimeRouteSummary, TaskStatusStage,
    TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::{task_status_storage_paths, TaskStatusStoragePaths};
use effigy_tasks::render_task_selector;

use super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;

pub(super) struct TaskStatusTracker {
    repo_root: std::path::PathBuf,
    key: TaskStatusKey,
    identity: TaskStatusTargetIdentity,
    execution_surface: ExecutionSurface,
    runtime_route: TaskStatusRuntimeRouteSummary,
    stage: TaskStatusStage,
    started_at: String,
    started_instant: Instant,
    lock_scopes: Vec<String>,
    active_path: std::path::PathBuf,
}

impl TaskStatusTracker {
    pub(super) fn start(
        preflight: &ExecutionPreflight,
        selection_plan: &ExecutionSelectionPlan,
        lock_scopes: Vec<String>,
    ) -> Result<Self, RunnerError> {
        let identity = TaskStatusTargetIdentity::new(
            preflight.resolved.resolved_root.clone(),
            selection_plan.catalog.catalog_root.clone(),
            render_task_selector(&preflight.selector),
            selection_plan.task_name.clone(),
            None,
        );
        let key = identity.status_key();
        let repo_root = preflight.resolved.resolved_root.clone();
        let started_at = timestamp_now();
        let paths = paths_for(&repo_root, &key, &started_at, TaskStatusState::Running);
        let tracker = Self {
            repo_root,
            key,
            identity,
            execution_surface: preflight.execution_surface.clone(),
            runtime_route: TaskStatusRuntimeRouteSummary {
                route: "pending".to_owned(),
                container: None,
                service: None,
            },
            stage: TaskStatusStage::WaitingForLock,
            started_at,
            started_instant: Instant::now(),
            lock_scopes,
            active_path: paths.active_path,
        };
        tracker.write_active_record()?;
        Ok(tracker)
    }

    pub(super) fn update_stage(
        &mut self,
        stage: TaskStatusStage,
        runtime_route: TaskStatusRuntimeRouteSummary,
    ) -> Result<(), RunnerError> {
        self.stage = stage;
        self.runtime_route = runtime_route;
        self.write_active_record()
    }

    pub(super) fn finish_success(self, summary: impl Into<String>) -> Result<(), RunnerError> {
        self.finish(
            TaskStatusState::Succeeded,
            TaskStatusOutcome {
                summary: summary.into(),
                error_family: None,
                error_code: None,
            },
        )
    }

    pub(super) fn finish_error(self, error: &RunnerError) -> Result<(), RunnerError> {
        let (state, outcome) = classify_error(error, self.stage);
        self.finish(state, outcome)
    }

    fn finish(self, state: TaskStatusState, outcome: TaskStatusOutcome) -> Result<(), RunnerError> {
        let finished_at = timestamp_now();
        let paths = paths_for(&self.repo_root, &self.key, &finished_at, state);
        let record = TaskStatusCompletedRecord {
            status_key: self.key.clone(),
            identity: self.identity.clone(),
            state,
            stage: Some(self.stage),
            execution_surface: self.execution_surface.clone(),
            runtime_route: self.runtime_route.clone(),
            started_at: self.started_at.clone(),
            finished_at,
            duration_ms: Some(duration_millis(self.started_instant.elapsed())),
            lock_scopes: self.lock_scopes.clone(),
            outcome,
            latest_report_path: display_path(&paths.latest_path, &self.repo_root),
            history_report_path: display_path(&paths.history_path, &self.repo_root),
        };
        write_json_file(&paths.latest_path, &record, "latest task-status record")?;
        write_json_file(&paths.history_path, &record, "task-status history record")?;
        match fs::remove_file(&self.active_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "failed to remove active task-status record `{}`: {error}",
                self.active_path.display()
            ))),
        }
    }

    fn write_active_record(&self) -> Result<(), RunnerError> {
        let record = TaskStatusActiveRecord {
            status_key: self.key.clone(),
            identity: self.identity.clone(),
            state: TaskStatusState::Running,
            stage: self.stage,
            execution_surface: self.execution_surface.clone(),
            runtime_route: self.runtime_route.clone(),
            owner_pid: std::process::id(),
            started_at: self.started_at.clone(),
            updated_at: timestamp_now(),
            lock_scopes: self.lock_scopes.clone(),
            active_record_path: display_path(&self.active_path, &self.repo_root),
        };
        write_json_file(&self.active_path, &record, "active task-status record")
    }
}

pub(super) fn pending_route_summary() -> TaskStatusRuntimeRouteSummary {
    TaskStatusRuntimeRouteSummary {
        route: "pending".to_owned(),
        container: None,
        service: None,
    }
}

pub(super) fn host_route_summary() -> TaskStatusRuntimeRouteSummary {
    TaskStatusRuntimeRouteSummary {
        route: "host".to_owned(),
        container: None,
        service: None,
    }
}

pub(super) fn inline_route_summary(container: &str) -> TaskStatusRuntimeRouteSummary {
    TaskStatusRuntimeRouteSummary {
        route: "inline-container".to_owned(),
        container: Some(container.to_owned()),
        service: None,
    }
}

pub(super) fn container_route_summary(
    container: &str,
    service: &str,
) -> TaskStatusRuntimeRouteSummary {
    TaskStatusRuntimeRouteSummary {
        route: "container".to_owned(),
        container: Some(container.to_owned()),
        service: Some(service.to_owned()),
    }
}

fn classify_error(
    error: &RunnerError,
    stage: TaskStatusStage,
) -> (TaskStatusState, TaskStatusOutcome) {
    match error {
        RunnerError::TaskCommandFailure { code, .. } if *code == Some(130) => (
            TaskStatusState::Cancelled,
            TaskStatusOutcome {
                summary: "task interrupted".to_owned(),
                error_family: Some("task-command-failure".to_owned()),
                error_code: Some("130".to_owned()),
            },
        ),
        RunnerError::TaskCommandFailure { code, .. } => (
            TaskStatusState::Failed,
            TaskStatusOutcome {
                summary: "task command failed".to_owned(),
                error_family: Some("task-command-failure".to_owned()),
                error_code: code.map(|value| value.to_string()),
            },
        ),
        RunnerError::CommandJsonFailure { .. } => (
            TaskStatusState::Failed,
            TaskStatusOutcome {
                summary: "task command failed".to_owned(),
                error_family: Some("command-json-failure".to_owned()),
                error_code: None,
            },
        ),
        RunnerError::TaskLockConflict(_) => (
            TaskStatusState::Blocked,
            TaskStatusOutcome {
                summary: "task blocked by active lock".to_owned(),
                error_family: Some("task-lock-conflict".to_owned()),
                error_code: None,
            },
        ),
        RunnerError::TaskCommandLaunch { .. } => (
            TaskStatusState::Blocked,
            TaskStatusOutcome {
                summary: "failed to launch task command".to_owned(),
                error_family: Some("task-command-launch".to_owned()),
                error_code: None,
            },
        ),
        RunnerError::TaskInvocation(message) => (
            blocked_or_failed(stage),
            TaskStatusOutcome {
                summary: message.clone(),
                error_family: Some("task-invocation".to_owned()),
                error_code: None,
            },
        ),
        other => (
            blocked_or_failed(stage),
            TaskStatusOutcome {
                summary: other.to_string(),
                error_family: Some("runner-error".to_owned()),
                error_code: None,
            },
        ),
    }
}

fn blocked_or_failed(stage: TaskStatusStage) -> TaskStatusState {
    match stage {
        TaskStatusStage::Executing
        | TaskStatusStage::ManagedSession
        | TaskStatusStage::Handoff
        | TaskStatusStage::Finishing => TaskStatusState::Failed,
        TaskStatusStage::Routing
        | TaskStatusStage::WaitingForLock
        | TaskStatusStage::RuntimePrep => TaskStatusState::Blocked,
    }
}

fn paths_for(
    repo_root: &Path,
    key: &TaskStatusKey,
    timestamp: &str,
    state: TaskStatusState,
) -> TaskStatusStoragePaths {
    task_status_storage_paths(repo_root, key, timestamp, state_slug(state))
}

fn state_slug(state: TaskStatusState) -> &'static str {
    match state {
        TaskStatusState::Running => "running",
        TaskStatusState::Succeeded => "succeeded",
        TaskStatusState::Failed => "failed",
        TaskStatusState::Cancelled => "cancelled",
        TaskStatusState::Blocked => "blocked",
        TaskStatusState::Unknown => "unknown",
    }
}

fn timestamp_now() -> String {
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn display_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn write_json_file(
    path: &Path,
    value: &impl serde::Serialize,
    label: &str,
) -> Result<(), RunnerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to create parent directory for {label} `{}`: {error}",
                path.display()
            ))
        })?;
    }
    let encoded = serde_json::to_string_pretty(value).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to encode {label} `{}`: {error}",
            path.display()
        ))
    })?;
    fs::write(path, encoded).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to write {label} `{}`: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{blocked_or_failed, classify_error};
    use crate::runner::error::RunnerError;
    use effigy_execution::{TaskStatusStage, TaskStatusState};

    #[test]
    fn lock_conflict_classifies_as_blocked() {
        let (state, outcome) = classify_error(
            &RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
                scope: "task:build".to_owned(),
                lock_path: "/tmp/repo/.effigy/locks/task-build.lock".into(),
                holder_pid: Some(42),
                holder_started_at_epoch_ms: Some(1),
                holder_heartbeat_at_epoch_ms: Some(1),
                holder_hostname: None,
                holder_workspace_root: None,
                remediation: "unlock".to_owned(),
            })),
            TaskStatusStage::WaitingForLock,
        );
        assert_eq!(state, TaskStatusState::Blocked);
        assert_eq!(outcome.error_family.as_deref(), Some("task-lock-conflict"));
    }

    #[test]
    fn exit_130_classifies_as_cancelled() {
        let (state, outcome) = classify_error(
            &RunnerError::TaskCommandFailure {
                command: "sh".to_owned(),
                code: Some(130),
                stdout: String::new(),
                stderr: String::new(),
            },
            TaskStatusStage::Executing,
        );
        assert_eq!(state, TaskStatusState::Cancelled);
        assert_eq!(outcome.error_code.as_deref(), Some("130"));
    }

    #[test]
    fn blocked_or_failed_tracks_stage_boundary() {
        assert_eq!(
            blocked_or_failed(TaskStatusStage::RuntimePrep),
            TaskStatusState::Blocked
        );
        assert_eq!(
            blocked_or_failed(TaskStatusStage::Executing),
            TaskStatusState::Failed
        );
    }
}
