use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod active;
pub mod browser;
pub mod execution;
pub mod process;
pub mod records;
pub mod runtime;

pub use active::{
    active_attempt_is_stop_requested, append_demo_terminal_input, append_demo_terminal_resize,
    clear_active_attempt_state, clear_resize_handoff, load_active_attempt,
    prepare_demo_input_handoff, prepare_demo_resize_handoff, read_active_attempt_record,
    read_recent_output_lines, register_active_attempt, resolve_repo_relative_path,
    update_active_terminal_resize, write_active_attempt_record, DemoActiveAttemptGuard,
    PersistedDemoActiveAttempt, PersistedDemoActivePhase, PersistedDemoTerminalTransport,
};
pub use execution::{
    failed_demo_attempt, persist_demo_attempt_logs, successful_demo_attempt,
    terminated_demo_attempt, write_latest_attempt_receipt, DemoExecutionAttempt, DemoLogPaths,
};
pub use process::{
    browser_terminal_size_override, current_terminal_size, demo_mode_prefers_attached_terminal,
    initial_terminal_size_for_launch_mode, resolve_demo_launch_mode, sanitize_pty_transcript,
    spawn_input_handoff_forward, spawn_output_capture, spawn_stdin_forward,
    spawn_stdin_handoff_capture, stop_input_handoff_forward, wrap_pty_shell_command,
    DemoInputHandoffForward, DemoLaunchMode, OutputMirror, DEMO_BROWSER_TERMINAL_COLS_ENV,
    DEMO_BROWSER_TERMINAL_ROWS_ENV, DEMO_DEFAULT_TERMINAL_COLS, DEMO_DEFAULT_TERMINAL_ROWS,
};
pub mod projection;
pub use records::{
    build_demo_groups, demo_run_preview, derive_gap_class, find_historical_attempt,
    history_attempt_to_json, history_attempts_with_limit, history_attempts_with_outcome,
    DemoActionAvailability, DemoEntrypoint, DemoGroup, DemoRecord, DemoRecordGroupBy,
};
pub use runtime::{
    concurrent_runner_input_target_process, concurrent_runner_projected_output_provenance,
    concurrent_runner_projected_process_summary, concurrent_runner_projection_shape,
    concurrent_runner_runtime_backend, concurrent_runner_supports_browser_live_attach,
    render_non_zero_exits, DemoConcurrentRuntimeState, DEMO_MANAGED_EVENT_POLL_INTERVAL_MS,
    DEMO_STREAM_DRAIN_POLLS_AFTER_EXIT,
};

use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_manifest::ManifestDemoConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

const DEMO_RECEIPTS_DIR: &str = ".effigy/demo/receipts";
const DEMO_ACTIVE_DIR: &str = ".effigy/demo/active";
const DEMO_LOGS_DIR: &str = ".effigy/demo/logs";
const DEMO_HISTORY_DIR: &str = ".effigy/demo/history";
pub const DEMO_ATTEMPT_HISTORY_LIMIT: usize = 10;

#[derive(Debug, Clone)]
pub struct DemoStateError {
    message: String,
}

impl DemoStateError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DemoStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DemoStateError {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoLatestAttempt {
    pub recorded: bool,
    pub receipt_path: Option<String>,
    pub outcome: Option<String>,
    pub summary: Option<String>,
    pub stale: bool,
    pub artifacts: Vec<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub parse_error: Option<String>,
}

impl DemoLatestAttempt {
    pub fn state_label(&self) -> &'static str {
        if self.recorded {
            "recorded"
        } else {
            "no-recorded-attempt"
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "recorded": self.recorded,
            "state": self.state_label(),
            "receipt_path": self.receipt_path,
            "receipt_present": self.receipt_path.is_some(),
            "outcome": self.outcome,
            "summary": self.summary,
            "freshness": if self.stale { "stale" } else { "current" },
            "stale": self.stale,
            "artifact_count": self.artifacts.len(),
            "artifacts": self.artifacts,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "output_available": self.stdout_log_path.is_some() || self.stderr_log_path.is_some(),
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoAttemptHistory {
    pub path: Option<String>,
    pub attempts: Vec<DemoHistoricalAttempt>,
    pub parse_error: Option<String>,
}

impl DemoAttemptHistory {
    pub fn to_json(&self) -> JsonValue {
        json!({
            "path": self.path,
            "count": self.attempts.len(),
            "attempts": self.attempts.iter().map(DemoHistoricalAttempt::to_json).collect::<Vec<_>>(),
            "parse_error": self.parse_error,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoHistoricalAttempt {
    pub attempt_id: String,
    pub recorded_at_epoch_ms: u128,
    pub outcome: String,
    pub summary: Option<String>,
    pub receipt_path: Option<String>,
    pub artifacts: Vec<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub exit_code: Option<i32>,
}

impl DemoHistoricalAttempt {
    pub fn from_persisted(value: PersistedDemoHistoricalAttempt) -> Self {
        Self {
            attempt_id: value.attempt_id,
            recorded_at_epoch_ms: value.recorded_at_epoch_ms,
            outcome: value.outcome,
            summary: value.summary,
            receipt_path: value.receipt_path,
            artifacts: value.artifacts,
            stdout_log_path: value.stdout_log_path,
            stderr_log_path: value.stderr_log_path,
            exit_code: value.exit_code,
        }
    }

    pub fn to_json(&self) -> JsonValue {
        json!({
            "attempt_id": self.attempt_id,
            "recorded_at_epoch_ms": self.recorded_at_epoch_ms,
            "outcome": self.outcome,
            "summary": self.summary,
            "receipt_path": self.receipt_path,
            "artifact_count": self.artifacts.len(),
            "artifacts": self.artifacts,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
            "exit_code": self.exit_code,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DemoAttemptRecord {
    pub outcome: String,
    pub summary: Option<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub exit_code: Option<i32>,
    pub recorded_at_epoch_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedDemoAttemptHistory {
    pub schema: String,
    pub schema_version: u8,
    pub demo_id: String,
    pub attempts: Vec<PersistedDemoHistoricalAttempt>,
}

impl PersistedDemoAttemptHistory {
    pub fn new(demo_id: &str) -> Self {
        Self {
            schema: "effigy.demo.attempt-history.v1".to_owned(),
            schema_version: 1,
            demo_id: demo_id.to_owned(),
            attempts: Vec::new(),
        }
    }

    pub fn push(&mut self, attempt: PersistedDemoHistoricalAttempt) {
        self.attempts.insert(0, attempt);
        self.attempts.truncate(DEMO_ATTEMPT_HISTORY_LIMIT);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedDemoHistoricalAttempt {
    pub attempt_id: String,
    pub recorded_at_epoch_ms: u128,
    pub outcome: String,
    pub summary: Option<String>,
    pub receipt_path: Option<String>,
    pub artifacts: Vec<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub exit_code: Option<i32>,
}

impl PersistedDemoHistoricalAttempt {
    pub fn from_record(
        demo_id: &str,
        demo: &ManifestDemoConfig,
        attempt: &DemoAttemptRecord,
        receipt_path: String,
    ) -> Self {
        Self {
            attempt_id: format!(
                "{}-{}",
                sanitize_demo_id_for_filename(demo_id),
                attempt.recorded_at_epoch_ms
            ),
            recorded_at_epoch_ms: attempt.recorded_at_epoch_ms,
            outcome: attempt.outcome.clone(),
            summary: attempt.summary.clone(),
            receipt_path: Some(receipt_path),
            artifacts: demo.artifacts.clone(),
            stdout_log_path: attempt.stdout_log_path.clone(),
            stderr_log_path: attempt.stderr_log_path.clone(),
            exit_code: attempt.exit_code,
        }
    }
}

pub fn load_latest_attempt(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> Result<DemoLatestAttempt, DemoStateError> {
    let receipt_path = effective_receipt_path(repo_root, demo_id, demo);
    let mut artifacts = demo.artifacts.clone();
    let rendered_receipt_path = display_repo_path(&receipt_path, repo_root);
    if !receipt_path.exists() {
        return Ok(DemoLatestAttempt {
            recorded: false,
            receipt_path: Some(rendered_receipt_path),
            outcome: None,
            summary: None,
            stale: false,
            artifacts,
            stdout_log_path: None,
            stderr_log_path: None,
            parse_error: None,
        });
    }

    let content = fs::read_to_string(&receipt_path)
        .map_err(|error| DemoStateError::new(failed_to_read_path(&receipt_path, error)))?;
    let parsed = match serde_json::from_str::<JsonValue>(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(DemoLatestAttempt {
                recorded: true,
                receipt_path: Some(rendered_receipt_path),
                outcome: None,
                summary: None,
                stale: false,
                artifacts,
                stdout_log_path: None,
                stderr_log_path: None,
                parse_error: Some(error.to_string()),
            });
        }
    };

    if let Some(receipt_artifacts) = normalize_artifact_refs(parsed.get("artifacts")) {
        for artifact in receipt_artifacts {
            if !artifacts.contains(&artifact) {
                artifacts.push(artifact);
            }
        }
    }

    Ok(DemoLatestAttempt {
        recorded: true,
        receipt_path: Some(rendered_receipt_path),
        outcome: parsed
            .get("status")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        summary: parsed
            .get("summary")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        stale: parsed
            .get("stale")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
            || parsed
                .get("freshness")
                .and_then(JsonValue::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case("stale")),
        artifacts,
        stdout_log_path: parsed
            .get("stdout_log_path")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        stderr_log_path: parsed
            .get("stderr_log_path")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        parse_error: None,
    })
}

pub fn load_attempt_history(
    repo_root: &Path,
    demo_id: &str,
) -> Result<DemoAttemptHistory, DemoStateError> {
    let path = effective_attempt_history_path(repo_root, demo_id);
    let rendered_path = display_repo_path(&path, repo_root);
    if !path.exists() {
        return Ok(DemoAttemptHistory {
            path: Some(rendered_path),
            attempts: Vec::new(),
            parse_error: None,
        });
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| DemoStateError::new(failed_to_read_path(&path, error)))?;
    let parsed = match serde_json::from_str::<PersistedDemoAttemptHistory>(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(DemoAttemptHistory {
                path: Some(rendered_path),
                attempts: Vec::new(),
                parse_error: Some(error.to_string()),
            });
        }
    };

    Ok(DemoAttemptHistory {
        path: Some(rendered_path),
        attempts: parsed
            .attempts
            .into_iter()
            .map(DemoHistoricalAttempt::from_persisted)
            .collect(),
        parse_error: None,
    })
}

pub fn append_attempt_history(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
    attempt: &DemoAttemptRecord,
) -> Result<(), DemoStateError> {
    let path = effective_attempt_history_path(repo_root, demo_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DemoStateError::new(failed_to_write_path(parent, error)))?;
    }

    let mut history = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| DemoStateError::new(failed_to_read_path(&path, error)))?;
        serde_json::from_str::<PersistedDemoAttemptHistory>(&content)
            .map_err(|error| DemoStateError::new(failed_to_parse_path(&path, error)))?
    } else {
        PersistedDemoAttemptHistory::new(demo_id)
    };

    history.push(PersistedDemoHistoricalAttempt::from_record(
        demo_id,
        demo,
        attempt,
        display_repo_path(&effective_receipt_path(repo_root, demo_id, demo), repo_root),
    ));

    let rendered = serde_json::to_string_pretty(&history)
        .map_err(|error| DemoStateError::new(failed_to_render_path(&path, error)))?;
    fs::write(&path, rendered)
        .map_err(|error| DemoStateError::new(failed_to_write_path(&path, error)))
}

pub fn effective_receipt_path(
    repo_root: &Path,
    demo_id: &str,
    demo: &ManifestDemoConfig,
) -> PathBuf {
    if let Some(path) = &demo.receipt {
        return repo_root.join(path);
    }
    repo_root
        .join(DEMO_RECEIPTS_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

pub fn effective_active_attempt_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root
        .join(DEMO_ACTIVE_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

pub fn effective_attempt_history_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root
        .join(DEMO_HISTORY_DIR)
        .join(format!("{}.json", sanitize_demo_id_for_filename(demo_id)))
}

pub fn effective_output_log_path(repo_root: &Path, demo_id: &str, stream: &str) -> PathBuf {
    repo_root.join(DEMO_LOGS_DIR).join(format!(
        "{}.{}.log",
        sanitize_demo_id_for_filename(demo_id),
        stream
    ))
}

pub fn effective_input_handoff_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root.join(DEMO_ACTIVE_DIR).join(format!(
        "{}.stdin.log",
        sanitize_demo_id_for_filename(demo_id)
    ))
}

pub fn effective_resize_handoff_path(repo_root: &Path, demo_id: &str) -> PathBuf {
    repo_root.join(DEMO_ACTIVE_DIR).join(format!(
        "{}.resize.jsonl",
        sanitize_demo_id_for_filename(demo_id)
    ))
}

pub fn render_active_attempt_path(repo_root: &Path, demo_id: &str) -> String {
    display_repo_path(
        &effective_active_attempt_path(repo_root, demo_id),
        repo_root,
    )
}

pub fn build_attempt_id(demo_id: &str) -> String {
    format!(
        "{}-{}",
        sanitize_demo_id_for_filename(demo_id),
        now_epoch_ms()
    )
}

pub fn sanitize_demo_id_for_filename(demo_id: &str) -> String {
    demo_id
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' => '_',
            _ => ch,
        })
        .collect()
}

pub fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub fn display_repo_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn normalize_artifact_refs(value: Option<&JsonValue>) -> Option<Vec<String>> {
    let entries = value?.as_array()?;
    let mut rendered = Vec::new();
    for entry in entries {
        match entry {
            JsonValue::String(path) if !path.trim().is_empty() => rendered.push(path.clone()),
            JsonValue::Object(map) => {
                if let Some(path) = map.get("path").and_then(JsonValue::as_str) {
                    if !path.trim().is_empty() {
                        rendered.push(path.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Some(rendered)
}

#[cfg(test)]
mod tests {
    use super::{
        append_attempt_history, build_attempt_id, effective_attempt_history_path,
        load_attempt_history, load_latest_attempt, DemoAttemptRecord, PersistedDemoAttemptHistory,
        PersistedDemoHistoricalAttempt, DEMO_ATTEMPT_HISTORY_LIMIT,
    };
    use effigy_manifest::{
        ManifestDemoConfig, ManifestDemoMode, ManifestDemoStatus, ManifestManagedRun,
    };
    use serde_json::json;
    use std::fs;

    fn demo_config() -> ManifestDemoConfig {
        ManifestDemoConfig {
            title: "Demo".to_owned(),
            summary: "Summary".to_owned(),
            proof: "Proof".to_owned(),
            owner: "owner".to_owned(),
            mode: ManifestDemoMode::Headless,
            status: ManifestDemoStatus::Ready,
            covers: vec!["coverage".to_owned()],
            tags: Vec::new(),
            artifacts: vec!["docs/base.md".to_owned()],
            receipt: None,
            task: Some("demo".to_owned()),
            run: None::<ManifestManagedRun>,
            prerequisites: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    fn temp_repo(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{name}-{}", build_attempt_id("demo")))
    }

    #[test]
    fn demo_attempt_history_push_keeps_newest_entries_within_limit() {
        let mut history = PersistedDemoAttemptHistory::new("demo");
        for index in 0..(DEMO_ATTEMPT_HISTORY_LIMIT + 2) {
            history.push(PersistedDemoHistoricalAttempt {
                attempt_id: format!("attempt-{index}"),
                recorded_at_epoch_ms: index as u128,
                outcome: "passed".to_owned(),
                summary: None,
                receipt_path: None,
                artifacts: Vec::new(),
                stdout_log_path: None,
                stderr_log_path: None,
                exit_code: Some(0),
            });
        }

        assert_eq!(history.attempts.len(), DEMO_ATTEMPT_HISTORY_LIMIT);
        assert_eq!(history.attempts[0].attempt_id, "attempt-11");
        assert_eq!(
            history.attempts[DEMO_ATTEMPT_HISTORY_LIMIT - 1].attempt_id,
            "attempt-2"
        );
    }

    #[test]
    fn load_latest_attempt_merges_receipt_artifacts_and_stale_flag() {
        let repo_root = temp_repo("effigy-demo-latest-attempt");
        fs::create_dir_all(&repo_root).expect("create temp repo root");
        let demo = demo_config();
        let receipt_path = repo_root.join(".effigy/demo/receipts/demo.json");
        fs::create_dir_all(receipt_path.parent().expect("receipt parent")).expect("create parent");
        fs::write(
            &receipt_path,
            serde_json::to_string_pretty(&json!({
                "status": "passed",
                "summary": "good",
                "freshness": "stale",
                "artifacts": [
                    "docs/extra.md",
                    { "path": "docs/second.md" },
                    { "path": "" }
                ],
                "stdout_log_path": ".effigy/demo/logs/demo.stdout.log",
            }))
            .expect("render receipt"),
        )
        .expect("write receipt");

        let latest = load_latest_attempt(&repo_root, "demo", &demo).expect("load latest");
        assert!(latest.recorded);
        assert!(latest.stale);
        assert_eq!(latest.outcome.as_deref(), Some("passed"));
        assert_eq!(
            latest.artifacts,
            vec![
                "docs/base.md".to_owned(),
                "docs/extra.md".to_owned(),
                "docs/second.md".to_owned()
            ]
        );
    }

    #[test]
    fn append_attempt_history_writes_and_reads_newest_first() {
        let repo_root = temp_repo("effigy-demo-attempt-history");
        fs::create_dir_all(&repo_root).expect("create temp repo root");
        let demo = demo_config();

        append_attempt_history(
            &repo_root,
            "demo",
            &demo,
            &DemoAttemptRecord {
                outcome: "passed".to_owned(),
                summary: Some("first".to_owned()),
                stdout_log_path: Some("stdout-1.log".to_owned()),
                stderr_log_path: None,
                exit_code: Some(0),
                recorded_at_epoch_ms: 1,
            },
        )
        .expect("append first");
        append_attempt_history(
            &repo_root,
            "demo",
            &demo,
            &DemoAttemptRecord {
                outcome: "failed".to_owned(),
                summary: Some("second".to_owned()),
                stdout_log_path: Some("stdout-2.log".to_owned()),
                stderr_log_path: None,
                exit_code: Some(1),
                recorded_at_epoch_ms: 2,
            },
        )
        .expect("append second");

        let history_path = effective_attempt_history_path(&repo_root, "demo");
        assert!(history_path.exists());
        let history = load_attempt_history(&repo_root, "demo").expect("load history");
        assert_eq!(history.attempts.len(), 2);
        assert_eq!(history.attempts[0].recorded_at_epoch_ms, 2);
        assert_eq!(history.attempts[0].summary.as_deref(), Some("second"));
        assert_eq!(history.attempts[1].recorded_at_epoch_ms, 1);
    }
}
