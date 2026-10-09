use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use effigy_execution::{
    ExecutionSelectionPlan, ExecutionSurface, TaskStatusActiveRecord, TaskStatusCompletedRecord,
    TaskStatusKey, TaskStatusOutcome, TaskStatusRuntimeRouteSummary, TaskStatusStage,
    TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::{
    reconcile_task_status_records, task_status_storage_paths, TaskStatusStoragePaths,
};
use effigy_tasks::{render_task_selector, TaskSurface};

use super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;

fn surface_for_execution(surface: &ExecutionSurface) -> TaskSurface {
    match surface {
        ExecutionSurface::Draft => TaskSurface::Draft,
        _ => TaskSurface::Published,
    }
}

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
    published_active: bool,
}

impl TaskStatusTracker {
    pub(super) fn start(
        preflight: &ExecutionPreflight,
        selection_plan: &ExecutionSelectionPlan,
        lock_scopes: Vec<String>,
    ) -> Result<Self, RunnerError> {
        let identity = TaskStatusTargetIdentity::new_on_surface(
            surface_for_execution(&preflight.execution_surface),
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
            runtime_route: pending_route_summary(),
            stage: TaskStatusStage::WaitingForLock,
            started_at,
            started_instant: Instant::now(),
            lock_scopes,
            active_path: paths.active_path,
            published_active: false,
        };
        Ok(tracker)
    }

    pub(super) fn update_stage(
        &mut self,
        stage: TaskStatusStage,
        runtime_route: TaskStatusRuntimeRouteSummary,
    ) -> Result<(), RunnerError> {
        self.stage = stage;
        self.runtime_route = runtime_route;
        self.published_active = true;
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
        write_json_file(&paths.history_path, &record, "task-status history record")?;
        if self.should_publish_latest_record()? {
            write_json_file(&paths.latest_path, &record, "latest task-status record")?;
        }
        if self.published_active {
            match fs::remove_file(&self.active_path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(RunnerError::task_invocation(format!(
                        "failed to remove active task-status record `{}`: {error}",
                        self.active_path.display()
                    )));
                }
            }
        }
        Ok(())
    }

    fn should_publish_latest_record(&self) -> Result<bool, RunnerError> {
        if self.published_active {
            return Ok(true);
        }
        let snapshot = reconcile_task_status_records(&self.repo_root, &self.key)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        Ok(snapshot.active.is_none())
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

static STATUS_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

type AfterStagedHook<'a> = dyn Fn(&Path, &Path) + 'a;

struct JsonPublicationHooks<'a> {
    after_staged: Option<&'a AfterStagedHook<'a>>,
    fail_before_publish: bool,
}

impl JsonPublicationHooks<'_> {
    const fn none() -> Self {
        Self {
            after_staged: None,
            fail_before_publish: false,
        }
    }
}

fn write_json_file(
    path: &Path,
    value: &impl serde::Serialize,
    label: &str,
) -> Result<(), RunnerError> {
    write_json_file_with_hooks(path, value, label, JsonPublicationHooks::none())
}

fn write_json_file_with_hooks(
    path: &Path,
    value: &impl serde::Serialize,
    label: &str,
    hooks: JsonPublicationHooks<'_>,
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
    let (temp_path, mut file) = create_status_temp_file(path, label)?;
    let staged = file
        .write_all(encoded.as_bytes())
        .and_then(|()| file.flush());
    drop(file);
    if let Err(error) = staged {
        let _ = fs::remove_file(&temp_path);
        return Err(json_write_error(path, label, error));
    }
    if let Some(after_staged) = hooks.after_staged {
        after_staged(&temp_path, path);
    }
    if hooks.fail_before_publish {
        let _ = fs::remove_file(&temp_path);
        return Err(RunnerError::task_invocation(format!(
            "failed to write {label} `{}`: publication probe refused rename",
            path.display()
        )));
    }
    match fs::rename(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            Err(json_write_error(path, label, error))
        }
    }
}

fn json_write_error(path: &Path, label: &str, error: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(format!(
        "failed to write {label} `{}`: {error}",
        path.display()
    ))
}

fn create_status_temp_file(path: &Path, label: &str) -> Result<(PathBuf, fs::File), RunnerError> {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("record");
    for _ in 0..256 {
        let counter = STATUS_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let candidate = path.with_file_name(format!(
            ".{filename}.effigy-status-{}-{nanos}-{counter}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(json_write_error(path, label, error)),
        }
    }
    Err(RunnerError::task_invocation(format!(
        "failed to allocate a unique staged task-status file for `{}`",
        path.display()
    )))
}

#[cfg(test)]
fn write_json_file_truncating_with_mid_write_pause(
    path: &Path,
    value: &impl serde::Serialize,
    label: &str,
    after_truncate: fn(&Path),
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
    let mut file = fs::File::create(path).map_err(|error| json_write_error(path, label, error))?;
    after_truncate(path);
    file.write_all(encoded.as_bytes())
        .map_err(|error| json_write_error(path, label, error))
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
                wait_timeout_ms: None,
                waited_ms: None,
                status_command: None,
                details_json: None,
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

    mod publication {
        use std::fs;
        use std::path::Path;

        use serde_json::json;

        use super::super::{
            write_json_file, write_json_file_truncating_with_mid_write_pause,
            write_json_file_with_hooks, JsonPublicationHooks,
        };

        fn status_temp_leftovers(dir: &Path) -> Vec<std::path::PathBuf> {
            let mut leftovers = Vec::new();
            let entries = match fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => return leftovers,
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                if name.contains(".effigy-status-") && name.ends_with(".tmp") {
                    leftovers.push(entry.path());
                }
            }
            leftovers
        }

        fn parse_error_is_empty_eof(error: &serde_json::Error) -> bool {
            let message = error.to_string();
            message.contains("EOF while parsing a value at line 1 column 0")
        }

        #[test]
        fn publication_truncating_write_exposes_empty_json_to_a_waiting_reader() {
            let temp = tempfile::tempdir().expect("tempdir");
            let path = temp.path().join("active.json");
            let value = json!({"state": "running", "generation": 1});

            write_json_file_truncating_with_mid_write_pause(
                &path,
                &value,
                "active task-status record",
                |path| {
                    let body = fs::read_to_string(path).expect("read truncated destination");
                    assert!(
                        body.is_empty(),
                        "pre-fix fs::write window must expose empty destination bytes, got {body:?}"
                    );
                    let error = serde_json::from_str::<serde_json::Value>(&body)
                        .expect_err("empty destination must fail closed");
                    assert!(
                        parse_error_is_empty_eof(&error),
                        "expected EOF line 1 column 0, got {error}"
                    );
                },
            )
            .expect("truncating write should finish");

            let complete = fs::read_to_string(&path).expect("read completed destination");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&complete).expect("complete json"),
                value
            );
        }

        #[test]
        fn publication_atomic_write_first_record_stays_absent_until_rename() {
            let temp = tempfile::tempdir().expect("tempdir");
            let path = temp.path().join("active.json");
            let value = json!({"state": "running", "generation": 1});

            write_json_file_with_hooks(
                &path,
                &value,
                "active task-status record",
                JsonPublicationHooks {
                    after_staged: Some(&|staged, destination| {
                        assert!(
                            staged.exists(),
                            "staged payload must exist before rename: {}",
                            staged.display()
                        );
                        assert!(
                            !destination.exists(),
                            "destination must stay absent until rename: {}",
                            destination.display()
                        );
                    }),
                    fail_before_publish: false,
                },
            )
            .expect("atomic first publication");

            let body = fs::read_to_string(&path).expect("read published destination");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).expect("published json"),
                value
            );
            assert!(
                status_temp_leftovers(temp.path()).is_empty(),
                "successful publication must retire the staged file"
            );
        }

        #[test]
        fn publication_atomic_write_keeps_old_record_visible_until_rename() {
            let temp = tempfile::tempdir().expect("tempdir");
            let path = temp.path().join("latest.json");
            let old = json!({"state": "succeeded", "generation": 1});
            let new = json!({"state": "succeeded", "generation": 2});
            write_json_file(&path, &old, "latest task-status record").expect("seed old record");
            let old_bytes = fs::read_to_string(&path).expect("old bytes");

            write_json_file_with_hooks(
                &path,
                &new,
                "latest task-status record",
                JsonPublicationHooks {
                    after_staged: Some(&|_, destination| {
                        let visible = fs::read_to_string(destination)
                            .expect("read destination during staged window");
                        assert_eq!(
                            visible, old_bytes,
                            "readers must keep the complete previous record until rename"
                        );
                    }),
                    fail_before_publish: false,
                },
            )
            .expect("atomic replacement");

            let body = fs::read_to_string(&path).expect("read replaced destination");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).expect("new json"),
                new
            );
            assert!(status_temp_leftovers(temp.path()).is_empty());
        }

        #[test]
        fn publication_failure_before_rename_preserves_old_record_and_removes_owned_temp() {
            let temp = tempfile::tempdir().expect("tempdir");
            let path = temp.path().join("latest.json");
            let old = json!({"state": "succeeded", "generation": 1});
            write_json_file(&path, &old, "latest task-status record").expect("seed old record");
            let old_bytes = fs::read_to_string(&path).expect("old bytes");

            let error = write_json_file_with_hooks(
                &path,
                &json!({"state": "failed", "generation": 2}),
                "latest task-status record",
                JsonPublicationHooks {
                    after_staged: None,
                    fail_before_publish: true,
                },
            )
            .expect_err("probe must refuse rename");
            assert!(
                error
                    .to_string()
                    .contains("publication probe refused rename"),
                "{error}"
            );
            assert_eq!(
                fs::read_to_string(&path).expect("preserved destination"),
                old_bytes
            );
            assert!(
                status_temp_leftovers(temp.path()).is_empty(),
                "failure must remove only the owned staged file"
            );
        }

        #[cfg(unix)]
        #[test]
        fn publication_destination_permissions_match_plain_write() {
            use std::os::unix::fs::PermissionsExt;

            let temp = tempfile::tempdir().expect("tempdir");
            let control = temp.path().join("control.json");
            let path = temp.path().join("active.json");
            fs::write(&control, "{\"ok\":true}").expect("plain write control");
            write_json_file(&path, &json!({"ok": true}), "active task-status record")
                .expect("atomic write");

            let expected = fs::metadata(&control)
                .expect("control metadata")
                .permissions();
            let actual = fs::metadata(&path)
                .expect("published metadata")
                .permissions();
            assert_eq!(
                actual.mode() & 0o777,
                expected.mode() & 0o777,
                "atomic publication must keep ordinary file permission bits"
            );
            assert!(
                fs::metadata(&path).expect("published metadata").is_file(),
                "published record must be a regular file"
            );
        }
    }
}
