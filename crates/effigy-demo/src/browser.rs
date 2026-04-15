use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

pub use crate::runtime::{
    DemoActiveTerminalSession, DemoRuntimeBackend, DemoRuntimeProjectedOutputProvenance,
    DemoRuntimeProjectedProcessSummary, DemoRuntimeProjectionShape, DemoTerminalInputForwarding,
    DemoTerminalRecentOutput, DemoTerminalResize, DemoTerminalSize,
};
use crate::DemoHistoricalAttempt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoListPayload {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub repo_root: String,
    pub query: JsonValue,
    pub group_by: Option<String>,
    pub count: usize,
    pub total_count: usize,
    #[serde(default)]
    pub groups: Option<Vec<DemoGroup>>,
    pub demos: Vec<DemoSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoGroup {
    pub label: String,
    pub count: usize,
    pub demos: Vec<DemoSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub owner: String,
    pub mode: String,
    pub status: String,
    pub effective_status: String,
    pub freshness: String,
    pub stale: bool,
    pub gap_class: String,
    pub covers: Vec<String>,
    pub tags: Vec<String>,
    pub entrypoint: DemoEntrypoint,
    pub defined_in: String,
    pub runtime_backend: DemoRuntimeBackend,
    pub actions: DemoActionAvailability,
    pub active_attempt: DemoActiveAttempt,
    pub active_terminal_session: DemoActiveTerminalSession,
    pub latest_attempt: DemoLatestAttempt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoInspectPayload {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub repo_root: String,
    pub demo: DemoDetail,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoDetail {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub proof: String,
    pub owner: String,
    pub mode: String,
    pub status: String,
    pub effective_status: String,
    pub freshness: String,
    pub stale: bool,
    pub gap_class: String,
    pub covers: Vec<String>,
    pub tags: Vec<String>,
    pub prerequisites: Vec<String>,
    pub dependencies: Vec<String>,
    pub entrypoint: DemoEntrypoint,
    pub defined_in: String,
    pub sources: Vec<String>,
    pub runtime_backend: DemoRuntimeBackend,
    pub actions: DemoActionAvailability,
    pub active_attempt: DemoActiveAttempt,
    pub active_terminal_session: DemoActiveTerminalSession,
    pub latest_attempt: DemoLatestAttempt,
    pub attempt_history: DemoAttemptHistory,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoAttemptHistory {
    pub path: Option<String>,
    pub count: usize,
    pub attempts: Vec<DemoHistoricalAttempt>,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoHistoryPayload {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub repo_root: String,
    pub query: JsonValue,
    pub demo: DemoHistoryDemo,
    pub active_attempt: DemoActiveAttempt,
    pub latest_attempt: DemoLatestAttempt,
    pub attempt_history: DemoHistoryAttemptHistoryPayload,
    #[serde(default)]
    pub selected_attempt: Option<DemoHistoricalAttempt>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoHistoryDemo {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub entrypoint: DemoEntrypoint,
    pub defined_in: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoHistoryAttemptHistoryPayload {
    pub path: Option<String>,
    pub stored_count: usize,
    pub filtered_count: usize,
    pub displayed_count: usize,
    pub count: usize,
    pub limit: Option<usize>,
    pub outcome: Option<String>,
    pub parse_error: Option<String>,
    pub attempts: Vec<DemoHistoryAttempt>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoHistoryAttempt {
    pub ordinal: usize,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoEntrypoint {
    pub kind: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoActionAvailability {
    pub run: DemoActionState,
    pub stop: DemoActionState,
    pub rerun: DemoActionState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoActionState {
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoEntrypointOptional {
    pub kind: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoActiveAttempt {
    pub active: bool,
    pub state: String,
    pub attempt_id: Option<String>,
    pub state_path: Option<String>,
    pub owner_pid: Option<u32>,
    pub target_pid: Option<u32>,
    pub stoppable: bool,
    pub started_at_epoch_ms: Option<u128>,
    pub entrypoint: DemoEntrypointOptional,
    pub command: Option<String>,
    pub runtime_backend: DemoRuntimeBackend,
    pub terminal_transport: String,
    pub supports_input_forwarding: bool,
    pub supports_resize: bool,
    pub nested_tui: bool,
    pub terminal_size: DemoTerminalSize,
    pub resize_handoff_path: Option<String>,
    pub stdin_input_path: Option<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub output_available: bool,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DemoLatestAttempt {
    pub recorded: bool,
    pub state: String,
    pub receipt_path: Option<String>,
    pub receipt_present: bool,
    pub outcome: Option<String>,
    pub summary: Option<String>,
    pub freshness: String,
    pub stale: bool,
    pub artifact_count: usize,
    pub artifacts: Vec<String>,
    pub stdout_log_path: Option<String>,
    pub stderr_log_path: Option<String>,
    pub output_available: bool,
    pub parse_error: Option<String>,
}
