use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::{
    StateApplyHookContext, StateApplyHookLayerContext, StateCaptureMode, StateCaptureTaskContext,
    StateHistoryKind, StateStackApplyLayerReport, StateStackLineageReport,
    STATE_STACK_APPLY_CONTEXT_SCHEMA, STATE_STACK_CAPTURE_CONTEXT_SCHEMA,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateReportWritePaths {
    pub compatibility_path: Option<PathBuf>,
    pub latest_path: PathBuf,
    pub history_path: PathBuf,
}

impl StateReportWritePaths {
    pub fn all_paths(&self) -> impl Iterator<Item = &Path> {
        self.compatibility_path
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(self.latest_path.as_path()))
            .chain(std::iter::once(self.history_path.as_path()))
    }
}

pub fn state_report_write_paths(
    repo_root: &Path,
    stack_name: &str,
    kind: StateHistoryKind,
    lineage: Option<&str>,
    compatibility_file: Option<&str>,
) -> StateReportWritePaths {
    let stack_dir = repo_root
        .join(".effigy")
        .join("reports")
        .join("state")
        .join(safe_path_component(stack_name));
    let latest_path = stack_dir.join(format!("latest-{kind}.json"));
    let history_path = state_history_report_path(
        &stack_dir,
        kind,
        &utc_basic_timestamp(SystemTime::now()),
        &short_safe_lineage(lineage.unwrap_or("lineage-unknown")),
    );
    let compatibility_path = compatibility_file.map(|file| stack_dir.join(file));
    StateReportWritePaths {
        compatibility_path,
        latest_path,
        history_path,
    }
}

pub fn state_capture_set_report_write_paths(
    repo_root: &Path,
    stack_name: &str,
    key: &str,
) -> StateReportWritePaths {
    let stack_dir = repo_root
        .join(".effigy")
        .join("reports")
        .join("state")
        .join(safe_path_component(stack_name));
    let latest_path = stack_dir.join("latest-capture-set.json");
    let history_path = stack_dir.join("history").join(format!(
        "{}-capture-set-{}.json",
        utc_compact_timestamp(SystemTime::now()),
        safe_path_component(key)
    ));
    StateReportWritePaths {
        compatibility_path: None,
        latest_path,
        history_path,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateContextFile<T> {
    pub relative_path: PathBuf,
    pub context: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateIoError {
    message: String,
}

impl StateIoError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for StateIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StateIoError {}

pub fn build_state_capture_task_context(
    lineage: &StateStackLineageReport,
    stack_name: &str,
    key: &str,
    capture_role: String,
    capture_mode: StateCaptureMode,
    source_environment: String,
    source: Option<String>,
    destination_ref: Option<String>,
) -> StateContextFile<StateCaptureTaskContext> {
    let relative_path = PathBuf::from(".effigy")
        .join("state")
        .join("capture-context")
        .join(safe_path_component(stack_name))
        .join(format!("{}.json", safe_path_component(key)));
    StateContextFile {
        relative_path,
        context: StateCaptureTaskContext {
            schema: STATE_STACK_CAPTURE_CONTEXT_SCHEMA.to_owned(),
            schema_version: 1,
            stack_name: lineage.stack_name.clone(),
            parent_lineage_id: lineage.lineage_id.clone(),
            capture_role,
            capture_mode: capture_mode.to_string(),
            source_environment,
            key: key.to_owned(),
            source,
            destination_ref,
        },
    }
}

pub fn build_state_apply_hook_context(
    stack_name: &str,
    environment: String,
    lineage_id: &str,
    layer: &StateStackApplyLayerReport,
    role: String,
    apply_mode: String,
) -> StateContextFile<StateApplyHookContext> {
    let relative_path = PathBuf::from(".effigy")
        .join("state")
        .join("apply-context")
        .join(safe_path_component(stack_name))
        .join(format!("{}.json", safe_path_component(&layer.key)));
    StateContextFile {
        relative_path,
        context: StateApplyHookContext {
            schema: STATE_STACK_APPLY_CONTEXT_SCHEMA.to_owned(),
            schema_version: 1,
            stack_name: stack_name.to_owned(),
            environment,
            lineage_id: lineage_id.to_owned(),
            layer: StateApplyHookLayerContext {
                index: layer.index,
                key: layer.key.clone(),
                role,
                apply_mode,
                source: layer.source.clone(),
                target: layer.target.clone(),
                hook: layer.hook.clone(),
                status: layer.status.to_string(),
                output: layer.output.clone(),
                artifact_report: layer.artifact_report.clone(),
                sql_report: layer.sql_report.clone(),
            },
        },
    }
}

pub fn write_state_report<T: Serialize>(
    repo_root: &Path,
    paths: &StateReportWritePaths,
    report: &T,
) -> Result<(), StateIoError> {
    let encoded = serde_json::to_string_pretty(report)
        .map_err(|error| StateIoError::new(error.to_string()))?;
    let encoded = format!("{encoded}\n");
    for path in paths.all_paths() {
        let Some(parent) = path.parent() else {
            return Err(StateIoError::new(format!(
                "failed to resolve parent directory for {}",
                path.display()
            )));
        };
        fs::create_dir_all(parent).map_err(|error| {
            StateIoError::new(format!(
                "failed to create state report directory {}: {error}",
                parent.display()
            ))
        })?;
        fs::write(path, &encoded).map_err(|error| {
            StateIoError::new(format!(
                "failed to write state report {}: {error}",
                path_display(path, repo_root)
            ))
        })?;
    }
    Ok(())
}

pub fn write_state_context_file<T: Serialize>(
    repo_root: &Path,
    context_file: &StateContextFile<T>,
    directory_label: &str,
    file_label: &str,
) -> Result<String, StateIoError> {
    let absolute_path = repo_root.join(&context_file.relative_path);
    let Some(parent) = absolute_path.parent() else {
        return Err(StateIoError::new(format!(
            "failed to resolve parent directory for {}",
            absolute_path.display()
        )));
    };
    fs::create_dir_all(parent).map_err(|error| {
        StateIoError::new(format!(
            "failed to create {directory_label} {}: {error}",
            parent.display()
        ))
    })?;
    let encoded = serde_json::to_string_pretty(&context_file.context)
        .map_err(|error| StateIoError::new(error.to_string()))?;
    fs::write(&absolute_path, format!("{encoded}\n")).map_err(|error| {
        StateIoError::new(format!(
            "failed to write {file_label} {}: {error}",
            path_display(&absolute_path, repo_root)
        ))
    })?;
    Ok(path_display(&absolute_path, repo_root))
}

pub fn resolve_repo_relative_path(repo_root: &Path, path: &str) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        path.display().to_string()
    } else {
        repo_root.join(path).display().to_string()
    }
}

fn state_history_report_path(
    stack_dir: &Path,
    kind: StateHistoryKind,
    timestamp: &str,
    lineage: &str,
) -> PathBuf {
    let history_dir = stack_dir.join("history");
    let base_name = format!("{timestamp}-{kind}-{lineage}");
    let mut path = history_dir.join(format!("{base_name}.json"));
    let mut suffix = 2;
    while path.exists() {
        path = history_dir.join(format!("{base_name}-{suffix}.json"));
        suffix += 1;
    }
    path
}

fn short_safe_lineage(lineage: &str) -> String {
    let safe = safe_path_component(lineage);
    safe.chars().take(48).collect()
}

fn utc_basic_timestamp(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = split_utc(time);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

fn utc_compact_timestamp(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = split_utc(time);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

fn split_utc(time: SystemTime) -> (i64, i64, i64, i64, i64, i64) {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_unix_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    (year, month, day, hour, minute, second)
}

fn civil_from_unix_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

pub fn safe_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

pub fn path_display(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}
