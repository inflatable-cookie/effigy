use std::fs;
use std::path::{Path, PathBuf};

use effigy_core::path_error_text::{failed_to_render_path, failed_to_write_path};
use effigy_manifest::ManifestDemoConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{
    append_attempt_history, display_repo_path, effective_output_log_path, effective_receipt_path,
    now_epoch_ms, DemoAttemptRecord, DemoStateError,
};

#[derive(Debug, Clone)]
pub struct DemoLogPaths {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_absolute: Option<PathBuf>,
    pub stderr_absolute: Option<PathBuf>,
}

impl DemoLogPaths {
    pub fn none() -> Self {
        Self {
            stdout: None,
            stderr: None,
            stdout_absolute: None,
            stderr_absolute: None,
        }
    }

    pub fn prepare_split(repo_root: &Path, demo_id: &str) -> Result<Self, DemoStateError> {
        let stdout_absolute = effective_output_log_path(repo_root, demo_id, "stdout");
        let stderr_absolute = effective_output_log_path(repo_root, demo_id, "stderr");
        if let Some(parent) = stdout_absolute.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
        }
        fs::write(&stdout_absolute, "")
            .map_err(|error| DemoStateError::new(failed_to_write_path(&stdout_absolute, error)))?;
        fs::write(&stderr_absolute, "")
            .map_err(|error| DemoStateError::new(failed_to_write_path(&stderr_absolute, error)))?;
        Ok(Self {
            stdout: Some(display_repo_path(&stdout_absolute, repo_root)),
            stderr: Some(display_repo_path(&stderr_absolute, repo_root)),
            stdout_absolute: Some(stdout_absolute),
            stderr_absolute: Some(stderr_absolute),
        })
    }

    pub fn prepare_pty(repo_root: &Path, demo_id: &str) -> Result<Self, DemoStateError> {
        let stdout_absolute = effective_output_log_path(repo_root, demo_id, "stdout");
        if let Some(parent) = stdout_absolute.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
        }
        fs::write(&stdout_absolute, "")
            .map_err(|error| DemoStateError::new(failed_to_write_path(&stdout_absolute, error)))?;
        Ok(Self {
            stdout: Some(display_repo_path(&stdout_absolute, repo_root)),
            stderr: None,
            stdout_absolute: Some(stdout_absolute),
            stderr_absolute: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoExecutionAttempt {
    pub ok: bool,
    pub outcome: String,
    pub entrypoint_kind: String,
    pub entrypoint_value: String,
    pub command: String,
    pub exit_code: Option<i32>,
    pub summary: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub recorded_at_epoch_ms: u128,
}

impl DemoExecutionAttempt {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "ok": self.ok,
            "outcome": self.outcome,
            "entrypoint": {
                "kind": self.entrypoint_kind,
                "value": self.entrypoint_value,
            },
            "command": self.command,
            "exit_code": self.exit_code,
            "summary": self.summary,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "recorded_at_epoch_ms": self.recorded_at_epoch_ms,
        })
    }
}

pub fn successful_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: Option<String>,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: true,
        outcome: "passed".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary,
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

pub fn failed_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: String,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: false,
        outcome: "failed".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary: Some(summary),
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

pub fn terminated_demo_attempt(
    entrypoint_kind: &str,
    entrypoint_value: &str,
    command: &str,
    exit_code: Option<i32>,
    summary: String,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    DemoExecutionAttempt {
        ok: false,
        outcome: "terminated".to_owned(),
        entrypoint_kind: entrypoint_kind.to_owned(),
        entrypoint_value: entrypoint_value.to_owned(),
        command: command.to_owned(),
        exit_code,
        summary: Some(summary),
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    }
}

pub fn write_latest_attempt_receipt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoExecutionAttempt,
) -> Result<(), DemoStateError> {
    let receipt_path = effective_receipt_path(repo_root, demo_id, demo);
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
    }

    let rendered = serde_json::to_string_pretty(&json!({
        "schema": "effigy.demo.receipt.v1",
        "schema_version": 1,
        "demo_id": demo_id,
        "ok": attempt.ok,
        "status": attempt.outcome,
        "summary": attempt.summary,
        "stale": false,
        "recorded_at_epoch_ms": attempt.recorded_at_epoch_ms,
        "entrypoint": {
            "kind": attempt.entrypoint_kind,
            "value": attempt.entrypoint_value,
        },
        "command": attempt.command,
        "exit_code": attempt.exit_code,
        "stdout_log_path": attempt.stdout_log_path,
        "stderr_log_path": attempt.stderr_log_path,
        "artifacts": demo.artifacts,
    }))
    .map_err(|error| DemoStateError::new(failed_to_render_path(&receipt_path, error)))?;

    fs::write(&receipt_path, rendered)
        .map_err(|error| DemoStateError::new(failed_to_write_path(&receipt_path, error)))?;
    append_attempt_history(
        repo_root,
        demo_id,
        demo,
        &DemoAttemptRecord {
            outcome: attempt.outcome.clone(),
            summary: attempt.summary.clone(),
            stdout_log_path: attempt.stdout_log_path.clone(),
            stderr_log_path: attempt.stderr_log_path.clone(),
            exit_code: attempt.exit_code,
            recorded_at_epoch_ms: attempt.recorded_at_epoch_ms,
        },
    )
}

pub fn persist_demo_attempt_logs(
    repo_root: &Path,
    demo_id: &str,
    stdout: &str,
    stderr: &str,
) -> Result<DemoLogPaths, DemoStateError> {
    if stdout.is_empty() && stderr.is_empty() {
        return Ok(DemoLogPaths::none());
    }
    let log_paths = DemoLogPaths::prepare_split(repo_root, demo_id)?;
    if let Some(path) = &log_paths.stdout_absolute {
        fs::write(path, stdout)
            .map_err(|error| DemoStateError::new(failed_to_write_path(path, error)))?;
    }
    if let Some(path) = &log_paths.stderr_absolute {
        fs::write(path, stderr)
            .map_err(|error| DemoStateError::new(failed_to_write_path(path, error)))?;
    }
    Ok(log_paths)
}

/// Kind of demo execution invocation — distinguishes `demo run` from `demo rerun`.
#[derive(Debug, Clone, Copy)]
pub enum DemoInvocationKind {
    Run,
    Rerun,
}

impl DemoInvocationKind {
    pub fn schema(&self) -> &'static str {
        match self {
            Self::Run => "effigy.demo.run.v1",
            Self::Rerun => "effigy.demo.rerun.v1",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Run => "Demo Run",
            Self::Rerun => "Demo Rerun",
        }
    }
}

/// Assemble a `DemoExecutionAttempt` from run-backed process output.
///
/// Selects the `terminated`, `passed`, or `failed` outcome shape based on
/// whether stop was requested and whether the run exited successfully.
pub fn run_attempt_from_output(
    demo_id: &str,
    entrypoint_value: &str,
    run_command: &str,
    exit_code: Option<i32>,
    success: bool,
    stop_requested: bool,
    stdout: String,
    stderr: String,
    log_paths: DemoLogPaths,
) -> DemoExecutionAttempt {
    if stop_requested {
        return terminated_demo_attempt(
            "run",
            entrypoint_value,
            run_command,
            exit_code,
            format!("Demo `{demo_id}` was terminated after stop was requested."),
            stdout,
            stderr,
            log_paths,
        );
    }
    if success {
        return successful_demo_attempt(
            "run",
            entrypoint_value,
            run_command,
            exit_code,
            Some(format!("Demo `{demo_id}` completed via run entrypoint.")),
            stdout,
            stderr,
            log_paths,
        );
    }
    failed_demo_attempt(
        "run",
        entrypoint_value,
        run_command,
        exit_code,
        format!("Demo `{demo_id}` failed via run entrypoint."),
        stdout,
        stderr,
        log_paths,
    )
}

/// Parse a task-backed demo attempt from a rendered json task payload.
///
/// Persists split stdout/stderr logs and returns a demo execution attempt
/// shaped from the parsed payload. Returns `DemoStateError` on parse or
/// log-persistence failure.
pub fn parse_task_backed_attempt_json(
    repo_root: &Path,
    demo_id: &str,
    task_name: &str,
    rendered: &str,
) -> Result<DemoExecutionAttempt, DemoStateError> {
    let parsed: JsonValue = serde_json::from_str(rendered).map_err(|error| {
        DemoStateError::new(format!(
            "failed to parse json task payload for demo `{demo_id}` task `{task_name}`: {error}"
        ))
    })?;
    let ok = parsed
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let exit_code = parsed
        .get("exit_code")
        .and_then(JsonValue::as_i64)
        .and_then(|value| i32::try_from(value).ok());
    let stdout = parsed
        .get("stdout")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let stderr = parsed
        .get("stderr")
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned();
    let command = parsed
        .get("command")
        .and_then(JsonValue::as_str)
        .unwrap_or(task_name)
        .to_owned();

    let log_paths = persist_demo_attempt_logs(repo_root, demo_id, &stdout, &stderr)?;

    Ok(DemoExecutionAttempt {
        ok,
        outcome: if ok {
            "passed".to_owned()
        } else {
            "failed".to_owned()
        },
        entrypoint_kind: "task".to_owned(),
        entrypoint_value: task_name.to_owned(),
        command,
        exit_code,
        summary: Some(if ok {
            format!("Demo `{demo_id}` completed via task `{task_name}`.")
        } else {
            format!("Demo `{demo_id}` failed via task `{task_name}`.")
        }),
        stdout,
        stderr,
        stdout_log_path: log_paths.stdout,
        stderr_log_path: log_paths.stderr,
        recorded_at_epoch_ms: now_epoch_ms(),
    })
}
