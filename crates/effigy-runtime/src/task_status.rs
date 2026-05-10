use std::path::{Path, PathBuf};

use effigy_execution::TaskStatusKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatusStoragePaths {
    pub active_path: PathBuf,
    pub latest_path: PathBuf,
    pub history_path: PathBuf,
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

fn task_status_history_dir(repo_root: &Path, key: &TaskStatusKey) -> PathBuf {
    repo_root
        .join(".effigy")
        .join("reports")
        .join("tasks")
        .join(key.as_str())
        .join("history")
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
    use std::path::PathBuf;

    use effigy_execution::TaskStatusTargetIdentity;

    use super::{
        task_status_active_record_path, task_status_history_record_path,
        task_status_latest_record_path, task_status_storage_paths,
    };

    fn key() -> effigy_execution::TaskStatusKey {
        TaskStatusTargetIdentity::new(
            PathBuf::from("/tmp/repo"),
            PathBuf::from("/tmp/repo/api"),
            "api/test",
            "test",
            None,
        )
        .status_key()
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
}
