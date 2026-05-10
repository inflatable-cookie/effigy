use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use effigy_execution::{
    TaskStatusActiveRecord, TaskStatusCompletedRecord, TaskStatusKey, TaskStatusWarning,
};
use nix::errno::Errno;
use nix::sys::signal;
use nix::unistd::Pid;

use crate::EffigyRuntimeError;

pub const TASK_STATUS_ACTIVE_STALE_MS: i64 = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatusStoragePaths {
    pub active_path: PathBuf,
    pub latest_path: PathBuf,
    pub history_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatusReadSnapshot {
    pub active: Option<TaskStatusActiveRecord>,
    pub latest: Option<TaskStatusCompletedRecord>,
    pub stale_active: Option<TaskStatusActiveRecord>,
    pub warnings: Vec<TaskStatusWarning>,
}

pub fn task_status_active_record_path(repo_root: &Path, key: &TaskStatusKey) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("runtime")
        .join("tasks")
        .join("active")
        .join(format!("{}.json", key.as_str()))
}

pub fn task_status_latest_record_path(repo_root: &Path, key: &TaskStatusKey) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("tasks")
        .join(key.as_str())
        .join("latest.json")
}

pub fn task_status_history_record_path(
    repo_root: &Path,
    key: &TaskStatusKey,
    timestamp: &str,
    suffix: &str,
) -> PathBuf {
    task_status_history_dir(repo_root, key).join(format!(
        "{}-{}.json",
        safe_path_component(timestamp),
        safe_path_component(suffix)
    ))
}

pub fn task_status_storage_paths(
    repo_root: &Path,
    key: &TaskStatusKey,
    timestamp: &str,
    suffix: &str,
) -> TaskStatusStoragePaths {
    TaskStatusStoragePaths {
        active_path: task_status_active_record_path(repo_root, key),
        latest_path: task_status_latest_record_path(repo_root, key),
        history_path: task_status_history_record_path(repo_root, key, timestamp, suffix),
    }
}

pub fn load_task_status_active_record(
    repo_root: &Path,
    key: &TaskStatusKey,
) -> Result<Option<TaskStatusActiveRecord>, EffigyRuntimeError> {
    load_json_file(
        task_status_active_record_path(repo_root, key),
        "active task-status record",
    )
}

pub fn load_task_status_latest_record(
    repo_root: &Path,
    key: &TaskStatusKey,
) -> Result<Option<TaskStatusCompletedRecord>, EffigyRuntimeError> {
    load_json_file(
        task_status_latest_record_path(repo_root, key),
        "latest task-status record",
    )
}

pub fn reconcile_task_status_records(
    repo_root: &Path,
    key: &TaskStatusKey,
) -> Result<TaskStatusReadSnapshot, EffigyRuntimeError> {
    let active = load_task_status_active_record(repo_root, key)?;
    let latest = load_task_status_latest_record(repo_root, key)?;
    let mut warnings = Vec::new();

    let (active, stale_active) = match active {
        Some(record) => {
            let stale_warnings = classify_active_record_staleness(&record);
            if stale_warnings.is_empty() {
                (Some(record), None)
            } else {
                warnings.extend(stale_warnings);
                (None, Some(record))
            }
        }
        None => (None, None),
    };

    Ok(TaskStatusReadSnapshot {
        active,
        latest,
        stale_active,
        warnings,
    })
}

pub fn list_task_status_keys(repo_root: &Path) -> Result<Vec<TaskStatusKey>, EffigyRuntimeError> {
    let mut keys = BTreeSet::new();
    collect_active_task_status_keys(repo_root, &mut keys)?;
    collect_latest_task_status_keys(repo_root, &mut keys)?;
    Ok(keys.into_iter().collect())
}

fn task_status_history_dir(repo_root: &Path, key: &TaskStatusKey) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("tasks")
        .join(key.as_str())
        .join("history")
}

fn collect_active_task_status_keys(
    repo_root: &Path,
    keys: &mut BTreeSet<TaskStatusKey>,
) -> Result<(), EffigyRuntimeError> {
    let active_root = repo_root
        .join(".effigy")
        .join("runtime")
        .join("tasks")
        .join("active");
    let entries = match fs::read_dir(&active_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to read active task-status directory `{}`: {error}",
                active_root.display()
            )))
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to read active task-status directory entry `{}`: {error}",
                active_root.display()
            ))
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if !stem.is_empty() {
            keys.insert(TaskStatusKey::from_storage_name(stem.to_owned()));
        }
    }
    Ok(())
}

fn collect_latest_task_status_keys(
    repo_root: &Path,
    keys: &mut BTreeSet<TaskStatusKey>,
) -> Result<(), EffigyRuntimeError> {
    let reports_root = repo_root.join(".effigy").join("reports").join("tasks");
    let entries = match fs::read_dir(&reports_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to read task-status reports directory `{}`: {error}",
                reports_root.display()
            )))
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to read task-status reports directory entry `{}`: {error}",
                reports_root.display()
            ))
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let latest = path.join("latest.json");
        if !latest.exists() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.is_empty() {
            keys.insert(TaskStatusKey::from_storage_name(name.to_owned()));
        }
    }
    Ok(())
}

fn classify_active_record_staleness(record: &TaskStatusActiveRecord) -> Vec<TaskStatusWarning> {
    let mut warnings = Vec::new();

    if !pid_is_alive(record.owner_pid) {
        warnings.push(TaskStatusWarning {
            code: "stale-active-pid-missing".to_owned(),
            message: format!(
                "active task-status record pid {} is no longer live",
                record.owner_pid
            ),
        });
    }

    match parse_status_timestamp(&record.updated_at) {
        Ok(updated_at) => {
            let age_ms = Utc::now()
                .signed_duration_since(updated_at)
                .num_milliseconds();
            if age_ms > TASK_STATUS_ACTIVE_STALE_MS {
                warnings.push(TaskStatusWarning {
                    code: "stale-active-heartbeat".to_owned(),
                    message: format!(
                        "active task-status record heartbeat is stale ({}ms old)",
                        age_ms
                    ),
                });
            }
        }
        Err(_) => warnings.push(TaskStatusWarning {
            code: "stale-active-updated-at-invalid".to_owned(),
            message: format!(
                "active task-status record has invalid updated_at `{}`",
                record.updated_at
            ),
        }),
    }

    warnings
}

fn load_json_file<T: serde::de::DeserializeOwned>(
    path: PathBuf,
    label: &str,
) -> Result<Option<T>, EffigyRuntimeError> {
    let encoded = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "failed to read {label} `{}`: {error}",
                path.display()
            )))
        }
    };

    serde_json::from_str(&encoded).map(Some).map_err(|error| {
        EffigyRuntimeError::task_invocation(format!(
            "failed to parse {label} `{}`: {error}",
            path.display()
        ))
    })
}

fn parse_status_timestamp(value: &str) -> Result<DateTime<Utc>, EffigyRuntimeError> {
    NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ")
        .map(|timestamp| timestamp.and_utc())
        .map_err(|error| {
            EffigyRuntimeError::task_invocation(format!(
                "failed to parse task-status timestamp `{value}`: {error}"
            ))
        })
}

fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    match signal::kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

fn safe_path_component(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        let mapped = match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => Some(ch.to_ascii_lowercase()),
            _ => None,
        };
        if let Some(ch) = mapped {
            output.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            output.push('-');
            last_was_dash = true;
        }
    }
    let output = output.trim_matches('-').to_owned();
    if output.is_empty() {
        "record".to_owned()
    } else {
        output
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::{DateTime, Utc};
    use effigy_execution::{
        ExecutionSurface, TaskStatusActiveRecord, TaskStatusCompletedRecord, TaskStatusKey,
        TaskStatusOutcome, TaskStatusRuntimeRouteSummary, TaskStatusStage, TaskStatusState,
        TaskStatusTargetIdentity,
    };
    use tempfile::tempdir;

    use super::{
        list_task_status_keys, load_task_status_active_record, load_task_status_latest_record,
        reconcile_task_status_records, task_status_active_record_path,
        task_status_history_record_path, task_status_latest_record_path, task_status_storage_paths,
        TASK_STATUS_ACTIVE_STALE_MS,
    };

    fn key() -> TaskStatusKey {
        TaskStatusTargetIdentity::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from("/tmp/repo/api"),
            "api/test",
            "test",
            None,
        )
        .status_key()
    }

    fn identity(repo_root: &Path) -> TaskStatusTargetIdentity {
        TaskStatusTargetIdentity::new(
            repo_root.to_path_buf(),
            repo_root.join("api"),
            "api/test",
            "test",
            None,
        )
    }

    fn route() -> TaskStatusRuntimeRouteSummary {
        TaskStatusRuntimeRouteSummary {
            route: "host".to_owned(),
            container: None,
            service: None,
        }
    }

    fn write_active(root: &Path, key: &TaskStatusKey, record: &TaskStatusActiveRecord) {
        let path = task_status_active_record_path(root, key);
        fs::create_dir_all(path.parent().expect("active parent")).expect("mkdir active");
        fs::write(path, serde_json::to_vec(record).expect("encode active")).expect("write active");
    }

    fn write_latest(root: &Path, key: &TaskStatusKey, record: &TaskStatusCompletedRecord) {
        let path = task_status_latest_record_path(root, key);
        fs::create_dir_all(path.parent().expect("latest parent")).expect("mkdir latest");
        fs::write(path, serde_json::to_vec(record).expect("encode latest")).expect("write latest");
    }

    fn active_record(
        root: &Path,
        key: &TaskStatusKey,
        updated_at: &str,
        owner_pid: u32,
    ) -> TaskStatusActiveRecord {
        TaskStatusActiveRecord {
            status_key: key.clone(),
            identity: identity(root),
            state: TaskStatusState::Running,
            stage: TaskStatusStage::Executing,
            execution_surface: ExecutionSurface::DirectCli,
            runtime_route: route(),
            owner_pid,
            started_at: "20260510T120000Z".to_owned(),
            updated_at: updated_at.to_owned(),
            lock_scopes: vec!["task:api/test".to_owned()],
            active_record_path: ".effigy/runtime/tasks/active/test.json".to_owned(),
        }
    }

    fn completed_record(root: &Path, key: &TaskStatusKey) -> TaskStatusCompletedRecord {
        TaskStatusCompletedRecord {
            status_key: key.clone(),
            identity: identity(root),
            state: TaskStatusState::Succeeded,
            stage: Some(TaskStatusStage::Executing),
            execution_surface: ExecutionSurface::DirectCli,
            runtime_route: route(),
            started_at: "20260510T120000Z".to_owned(),
            finished_at: "20260510T120001Z".to_owned(),
            duration_ms: Some(1000),
            lock_scopes: vec!["task:api/test".to_owned()],
            outcome: TaskStatusOutcome {
                summary: "ok".to_owned(),
                error_family: None,
                error_code: None,
            },
            latest_report_path: ".effigy/reports/tasks/test/latest.json".to_owned(),
            history_report_path: ".effigy/reports/tasks/test/history/record.json".to_owned(),
        }
    }

    fn now_timestamp() -> String {
        Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
    }

    fn stale_timestamp() -> String {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix time")
            .as_millis() as i64;
        let stale_ms = now_ms - TASK_STATUS_ACTIVE_STALE_MS - 5_000;
        let secs = stale_ms.div_euclid(1_000);
        DateTime::<Utc>::from_timestamp(secs, 0)
            .expect("timestamp")
            .format("%Y%m%dT%H%M%SZ")
            .to_string()
    }

    #[test]
    fn task_status_paths_follow_runtime_and_report_layout() {
        let repo_root = PathBuf::from("/tmp/repo");
        let key = key();

        assert_eq!(
            task_status_active_record_path(&repo_root, &key),
            repo_root
                .join(".effigy/runtime/tasks/active")
                .join(format!("{}.json", key.as_str()))
        );
        assert_eq!(
            task_status_latest_record_path(&repo_root, &key),
            repo_root
                .join(".effigy/reports/tasks")
                .join(key.as_str())
                .join("latest.json")
        );
        assert_eq!(
            task_status_history_record_path(&repo_root, &key, "20260510T112233Z", "succeeded"),
            repo_root
                .join(".effigy/reports/tasks")
                .join(key.as_str())
                .join("history")
                .join("20260510t112233z-succeeded.json")
        );
    }

    #[test]
    fn task_status_storage_paths_bundle_active_latest_and_history_locations() {
        let repo_root = PathBuf::from("/tmp/repo");
        let key = key();
        let paths = task_status_storage_paths(&repo_root, &key, "20260510T112233Z", "failed run");
        assert_eq!(
            paths.active_path,
            task_status_active_record_path(&repo_root, &key)
        );
        assert_eq!(
            paths.latest_path,
            task_status_latest_record_path(&repo_root, &key)
        );
        assert_eq!(
            paths.history_path,
            repo_root
                .join(".effigy/reports/tasks")
                .join(key.as_str())
                .join("history")
                .join("20260510t112233z-failed-run.json")
        );
    }

    #[test]
    fn load_helpers_read_active_and_latest_records() {
        let temp = tempdir().expect("tempdir");
        let repo_root = temp.path();
        let key = key();
        let active = active_record(repo_root, &key, &now_timestamp(), std::process::id());
        let latest = completed_record(repo_root, &key);
        write_active(repo_root, &key, &active);
        write_latest(repo_root, &key, &latest);

        assert_eq!(
            load_task_status_active_record(repo_root, &key).expect("load active"),
            Some(active)
        );
        assert_eq!(
            load_task_status_latest_record(repo_root, &key).expect("load latest"),
            Some(latest)
        );
    }

    #[test]
    fn reconcile_keeps_live_active_record() {
        let temp = tempdir().expect("tempdir");
        let repo_root = temp.path();
        let key = key();
        let active = active_record(repo_root, &key, &now_timestamp(), std::process::id());
        write_active(repo_root, &key, &active);

        let snapshot = reconcile_task_status_records(repo_root, &key).expect("reconcile");
        assert_eq!(snapshot.active, Some(active));
        assert!(snapshot.stale_active.is_none());
        assert!(snapshot.warnings.is_empty());
    }

    #[test]
    fn reconcile_marks_missing_pid_active_record_as_stale_and_falls_back_to_latest() {
        let temp = tempdir().expect("tempdir");
        let repo_root = temp.path();
        let key = key();
        let active = active_record(repo_root, &key, &now_timestamp(), 999_999);
        let latest = completed_record(repo_root, &key);
        write_active(repo_root, &key, &active);
        write_latest(repo_root, &key, &latest);

        let snapshot = reconcile_task_status_records(repo_root, &key).expect("reconcile");
        assert!(snapshot.active.is_none());
        assert_eq!(snapshot.latest, Some(latest));
        assert_eq!(snapshot.stale_active, Some(active));
        assert_eq!(snapshot.warnings.len(), 1);
        assert_eq!(snapshot.warnings[0].code, "stale-active-pid-missing");
    }

    #[test]
    fn reconcile_marks_stale_heartbeat_active_record_as_stale() {
        let temp = tempdir().expect("tempdir");
        let repo_root = temp.path();
        let key = key();
        let active = active_record(repo_root, &key, &stale_timestamp(), std::process::id());
        write_active(repo_root, &key, &active);

        let snapshot = reconcile_task_status_records(repo_root, &key).expect("reconcile");
        assert!(snapshot.active.is_none());
        assert_eq!(snapshot.stale_active, Some(active));
        assert_eq!(snapshot.warnings.len(), 1);
        assert_eq!(snapshot.warnings[0].code, "stale-active-heartbeat");
    }

    #[test]
    fn list_task_status_keys_collects_active_and_latest_without_duplicates() {
        let temp = tempdir().expect("tempdir");
        let repo_root = temp.path();
        let key = key();
        let active = active_record(repo_root, &key, &now_timestamp(), std::process::id());
        let latest = completed_record(repo_root, &key);
        write_active(repo_root, &key, &active);
        write_latest(repo_root, &key, &latest);

        let keys = list_task_status_keys(repo_root).expect("list task status keys");
        assert_eq!(keys, vec![key]);
    }
}
