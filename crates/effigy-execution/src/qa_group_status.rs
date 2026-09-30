//! Bounded QA-group run records (contract 051 outcome/JSON model).
//!
//! One record covers one run identity: the definition snapshot, head/worktree
//! context, scope result, admission and timing evidence, and the per-member
//! ledger. Records are written by the runner coordinator and read by the
//! `tasks qa-group status/logs` surfaces. Group identity is separate from
//! task/draft status identity and is never joined to it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Versioned initial/final run-record payload schema.
pub const QA_GROUP_RUN_SCHEMA: &str = "effigy.qa-group-run.v1";
/// Versioned live/reconciled status payload schema.
pub const QA_GROUP_STATUS_SCHEMA: &str = "effigy.qa-group-status.v1";

/// Aggregate group outcome, separate from budget evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaGroupOutcome {
    /// All selected members completed successfully.
    Passed,
    /// A member ran and failed.
    Failed,
    /// The owner or operator cancelled the run.
    Cancelled,
    /// An explicitly configured execution deadline fired. Unavailable until
    /// run-scoped stop/signal attribution (lead `29e5f6f7`) lands.
    TimedOut,
    /// Selection or a required runtime route failed before execution.
    Blocked,
    /// The admission deadline elapsed before execution began.
    CapacityTimeout,
    /// Persisted evidence cannot prove a live or complete result. Never
    /// reported as a pass.
    Unknown,
}

impl QaGroupOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Blocked => "blocked",
            Self::CapacityTimeout => "capacity_timeout",
            Self::Unknown => "unknown",
        }
    }
}

/// Per-member ledger states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaGroupMemberState {
    Passed,
    Failed,
    Cancelled,
    TimedOut,
    Blocked,
    /// Never executed because an earlier member failed or the run ended
    /// first. No pass is ever synthesized for these.
    NotStarted,
}

impl QaGroupMemberState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Blocked => "blocked",
            Self::NotStarted => "not_started",
        }
    }
}

/// Budget comparison evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaGroupBudgetState {
    WithinBudget,
    OverBudget,
    Unknown,
}

impl QaGroupBudgetState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WithinBudget => "within_budget",
            Self::OverBudget => "over_budget",
            Self::Unknown => "unknown",
        }
    }
}

/// Live run progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaGroupRunState {
    /// Waiting for host-wide heavy admission; no member has started.
    WaitingForCapacity,
    /// Admission granted; members are running.
    Running,
    /// Terminal: the final record was written.
    Completed,
    /// The live owner is gone without a final record; evidence is incomplete.
    Unknown,
}

impl QaGroupRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WaitingForCapacity => "waiting_for_capacity",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Unknown => "unknown",
        }
    }
}

/// Where and how the definition was captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupRunGroupSnapshot {
    pub surface: String,
    pub name: String,
    pub catalog: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_sha256: Option<String>,
    pub scope_policy: String,
    pub coverage_gaps: Vec<QaGroupGapSnapshot>,
    #[serde(default)]
    pub lifecycle: String,
    #[serde(default)]
    pub expired: bool,
}

/// One scope token's comparison result, frozen into the run record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupScopeMatchRecord {
    pub input: String,
    pub member_ids: Vec<String>,
    pub gap_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupGapSnapshot {
    pub input: String,
    pub reason: String,
}

/// Repository context. Context is not proof of a clean or immutable input
/// snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupRunHead {
    pub commit: Option<String>,
    pub worktree: String,
}

/// Wall-time evidence. Null means unavailable, never zero; every field is
/// always present so consumers can distinguish null from absent.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QaGroupRunTiming {
    #[serde(default)]
    pub queued_at: Option<String>,
    #[serde(default)]
    pub admitted_at: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub admission_wait_ms: Option<u64>,
    #[serde(default)]
    pub execution_wall_ms: Option<u64>,
    #[serde(default)]
    pub expected_wall_ms: Option<u64>,
    /// Always null until the runtime measures phase costs.
    #[serde(default)]
    pub cold_build_ms: Option<u64>,
    #[serde(default)]
    pub warm_build_ms: Option<u64>,
}

/// One member's record in the run ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupMemberRecord {
    pub id: String,
    pub kind: String,
    pub surface: String,
    pub catalog: String,
    pub selector: String,
    pub args: Vec<String>,
    pub targets: Vec<String>,
    pub covers: Vec<String>,
    pub state: QaGroupMemberState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_ref: Option<String>,
    /// Why the member never ran; present only with `not_started`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_started_reason: Option<String>,
    /// How the member ended, as a short classification for text output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// The complete run record: snapshot plus ledger. Written incrementally so a
/// live `status` can follow members, and finalized once at completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupRunRecord {
    /// Always `effigy.qa-group-run.v1`.
    pub schema: String,
    pub schema_version: u8,
    pub run_id: String,
    pub group: QaGroupRunGroupSnapshot,
    pub head: QaGroupRunHead,
    pub selected_targets: Vec<String>,
    pub scope_inputs: Vec<String>,
    /// Live runs carry only the explicit input list and the comparison
    /// result; `needs_planner` never creates a run.
    pub scope_assessment: String,
    pub scope_matches: Vec<QaGroupScopeMatchRecord>,
    /// Repeated on every run record: declared mappings are not proof of map
    /// truth or caller scope completeness.
    pub coverage_disclaimer: String,
    pub state: QaGroupRunState,
    /// Null until the run reaches a terminal aggregate state.
    #[serde(default)]
    pub outcome: Option<QaGroupOutcome>,
    pub budget_state: QaGroupBudgetState,
    pub timing: QaGroupRunTiming,
    /// Owner identity for staleness reconciliation; a reused PID without the
    /// recorded start identity never counts as live evidence.
    pub owner_pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_start_identity: Option<String>,
    #[serde(default)]
    pub boot_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// When the live owner last refreshed the record.
    pub members: Vec<QaGroupMemberRecord>,
    /// Run-scoped log directory relative to the repository root.
    pub log_dir: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// The plan's capability declaration, repeated for honest records.
    pub capabilities: QaGroupRunCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupRunCapabilities {
    pub hard_timeout: bool,
    pub stop: bool,
}

/// Classification for one member's exit, derived by the runner from the
/// canonical pipeline result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QaMemberExit {
    Passed,
    Failed,
    Cancelled,
}

/// The status read model: a run record plus reconciliation evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QaGroupStatusSnapshot {
    pub run_id: String,
    /// Path of the record that was read, relative to the repository root.
    pub record_path: String,
    /// True when the record came from the live runtime area.
    pub live: bool,
    pub record: QaGroupRunRecord,
    /// Set when the live owner is gone and no final record exists; the state
    /// is then `unknown`, never passed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Directory layout helper for consumers that render log paths.
pub fn member_log_file_name(member_id: &str) -> String {
    let safe: String = member_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("{safe}.log")
}

/// Default run-record path helper for status consumers.
pub fn qa_group_log_dir(repo_root: PathBuf, run_id: &str) -> PathBuf {
    repo_root.join(".effigy/reports/qa-groups").join(run_id)
}

#[cfg(test)]
mod tests {
    use super::{
        member_log_file_name, QaGroupBudgetState, QaGroupMemberState, QaGroupOutcome,
        QaGroupRunState,
    };

    #[test]
    fn state_names_are_stable_snake_case() {
        assert_eq!(QaGroupOutcome::CapacityTimeout.as_str(), "capacity_timeout");
        assert_eq!(QaGroupMemberState::NotStarted.as_str(), "not_started");
        assert_eq!(
            QaGroupRunState::WaitingForCapacity.as_str(),
            "waiting_for_capacity"
        );
        assert_eq!(QaGroupBudgetState::OverBudget.as_str(), "over_budget");
    }

    #[test]
    fn log_file_names_stay_filesystem_safe() {
        assert_eq!(member_log_file_name("cli-tests"), "cli-tests.log");
        assert_eq!(member_log_file_name("weird id/.."), "weird_id___.log");
    }
}
