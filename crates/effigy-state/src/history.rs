use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    path_display, safe_path_component, STATE_STACK_APPLY_SCHEMA, STATE_STACK_CAPTURE_SCHEMA,
    STATE_STACK_CAPTURE_SET_SCHEMA, STATE_STACK_HISTORY_SCHEMA, STATE_STACK_LINEAGE_SCHEMA,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackHistoryReport {
    pub schema: String,
    pub schema_version: u8,
    pub stack_name: String,
    pub reports: Vec<StateStackHistoryItem>,
    pub warnings: Vec<String>,
}

impl StateStackHistoryReport {
    pub fn scan(
        repo_root: &Path,
        stack: &str,
        kind: Option<StateHistoryKind>,
        limit: usize,
        lineage: Option<&str>,
    ) -> Self {
        let mut warnings = Vec::new();
        let stack_dir = repo_root
            .join(".effigy")
            .join("reports")
            .join("state")
            .join(safe_path_component(stack));
        let mut candidates = Vec::new();
        collect_state_history_candidates(&stack_dir, &mut candidates, &mut warnings);
        collect_state_history_candidates(
            &stack_dir.join("history"),
            &mut candidates,
            &mut warnings,
        );

        let mut reports = Vec::new();
        for path in candidates {
            match read_state_history_item(repo_root, &path) {
                Ok(Some(item)) => {
                    if kind.is_some_and(|expected| item.kind != expected) {
                        continue;
                    }
                    if let Some(lineage) = lineage {
                        let matches_lineage = item.lineage_id.as_deref() == Some(lineage)
                            || item.parent_lineage_id.as_deref() == Some(lineage);
                        if !matches_lineage {
                            continue;
                        }
                    }
                    reports.push(item);
                }
                Ok(None) => {}
                Err(error) => warnings.push(error),
            }
        }
        reports.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.path.cmp(&left.path))
        });
        reports.truncate(limit);

        Self {
            schema: STATE_STACK_HISTORY_SCHEMA.to_owned(),
            schema_version: 1,
            stack_name: stack.to_owned(),
            reports,
            warnings,
        }
    }
}

fn collect_state_history_candidates(
    dir: &Path,
    candidates: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.is_file()
                    && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                {
                    candidates.push(path);
                }
            }
            Err(error) => warnings.push(format!(
                "failed to read state history entry in {}: {error}",
                dir.display()
            )),
        }
    }
}

fn read_state_history_item(
    repo_root: &Path,
    path: &Path,
) -> Result<Option<StateStackHistoryItem>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("failed to read state report {}: {error}", path.display()))?;
    // Typed record: a field with the wrong JSON type is a malformed row
    // (reported as ignored) instead of silently degrading to a missing
    // fact. Optional fields stay `Option` so absent facts remain absent;
    // only the `created_at` fallback still substitutes the filename stem.
    let record: RawStateHistoryRecord = serde_json::from_str(&raw)
        .map_err(|error| format!("ignored malformed state report {}: {error}", path.display()))?;
    let schema = record.schema.clone().unwrap_or_default();
    let Some(kind) =
        StateHistoryKind::from_schema(&schema).or_else(|| StateHistoryKind::from_path(path))
    else {
        return Ok(None);
    };
    let created_at = record
        .created_at
        .clone()
        .unwrap_or_else(|| path_created_at_fallback(path));
    let summary = state_history_summary_typed(kind, &record);
    Ok(Some(StateStackHistoryItem {
        kind,
        schema,
        path: path_display(path, repo_root),
        created_at,
        lineage_id: record.lineage_id,
        parent_lineage_id: record.parent_lineage_id,
        ok: record.ok,
        executed: record.executed,
        summary,
    }))
}

/// Typed view of one persisted state report row.
///
/// Only the fields `state history` projects are decoded; unknown fields
/// are ignored for forward compatibility. Every field is optional so an
/// absent fact stays absent, while a present-but-wrongly-typed field fails
/// deserialization and the row is reported as malformed.
#[derive(Debug, Deserialize)]
struct RawStateHistoryRecord {
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    lineage_id: Option<String>,
    #[serde(default)]
    parent_lineage_id: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    executed: Option<bool>,
    #[serde(default)]
    layers: Option<Vec<Value>>,
    #[serde(default)]
    captures: Option<Vec<Value>>,
    #[serde(default)]
    produced_layers: Option<Vec<Value>>,
}

fn path_created_at_fallback(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

fn state_history_summary_typed(kind: StateHistoryKind, record: &RawStateHistoryRecord) -> String {
    match kind {
        StateHistoryKind::Plan => record
            .layers
            .as_ref()
            .map(|layers| format!("{} planned layer(s)", layers.len()))
            .unwrap_or_else(|| "plan report".to_owned()),
        StateHistoryKind::Apply => record
            .layers
            .as_ref()
            .map(|layers| format!("{} apply layer(s)", layers.len()))
            .unwrap_or_else(|| "apply report".to_owned()),
        StateHistoryKind::Capture => record
            .captures
            .as_ref()
            .map(|captures| format!("{} capture set item(s)", captures.len()))
            .or_else(|| {
                record
                    .produced_layers
                    .as_ref()
                    .map(|layers| format!("{} produced layer(s)", layers.len()))
            })
            .unwrap_or_else(|| "capture report".to_owned()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateHistoryKind {
    Plan,
    Apply,
    Capture,
}

impl StateHistoryKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "plan" => Some(Self::Plan),
            "apply" => Some(Self::Apply),
            "capture" => Some(Self::Capture),
            _ => None,
        }
    }

    fn from_schema(schema: &str) -> Option<Self> {
        match schema {
            STATE_STACK_LINEAGE_SCHEMA => Some(Self::Plan),
            STATE_STACK_APPLY_SCHEMA => Some(Self::Apply),
            STATE_STACK_CAPTURE_SCHEMA | STATE_STACK_CAPTURE_SET_SCHEMA => Some(Self::Capture),
            _ => None,
        }
    }

    fn from_path(path: &Path) -> Option<Self> {
        let file_name = path.file_name()?.to_str()?;
        if file_name.contains("plan") {
            Some(Self::Plan)
        } else if file_name.contains("apply") {
            Some(Self::Apply)
        } else if file_name.contains("capture") {
            Some(Self::Capture)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateHistoryKindParseError {
    value: String,
}

impl StateHistoryKindParseError {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

impl fmt::Display for StateHistoryKindParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "`state history --kind` must be `plan`, `apply`, or `capture`, got `{}`",
            self.value
        )
    }
}

impl std::error::Error for StateHistoryKindParseError {}

pub fn parse_state_history_kind(
    value: &str,
) -> Result<StateHistoryKind, StateHistoryKindParseError> {
    StateHistoryKind::parse(value).ok_or_else(|| StateHistoryKindParseError::new(value))
}

impl fmt::Display for StateHistoryKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Plan => "plan",
            Self::Apply => "apply",
            Self::Capture => "capture",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackHistoryItem {
    pub kind: StateHistoryKind,
    pub schema: String,
    pub path: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_lineage_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed: Option<bool>,
    pub summary: String,
}
