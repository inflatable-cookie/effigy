use std::fmt;

use serde::Serialize;
use serde_json::Value;

use crate::{
    StateEnvironment, StateLayerApplyMode, StateLayerRole, StateStackLineageReport,
    STATE_STACK_APPLY_SCHEMA,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStackApplyReport {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub executed: bool,
    pub stack_name: String,
    pub environment: StateEnvironment,
    pub lineage_id: String,
    pub layers: Vec<StateStackApplyLayerReport>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_history_path: Option<String>,
}

impl StateStackApplyReport {
    pub fn from_lineage(lineage: &StateStackLineageReport, execute: bool) -> Self {
        let layers = lineage
            .layers
            .iter()
            .map(|layer| {
                let status = match layer.apply_mode {
                    StateLayerApplyMode::Task if execute => StateStackApplyLayerStatus::PlannedTask,
                    StateLayerApplyMode::Artifact if execute => {
                        StateStackApplyLayerStatus::PlannedArtifactStage
                    }
                    StateLayerApplyMode::Sql if execute => {
                        StateStackApplyLayerStatus::PlannedSqlImport
                    }
                    StateLayerApplyMode::Task => StateStackApplyLayerStatus::WouldExecute,
                    StateLayerApplyMode::Artifact => StateStackApplyLayerStatus::WouldStage,
                    StateLayerApplyMode::Sql => StateStackApplyLayerStatus::WouldImport,
                    _ => StateStackApplyLayerStatus::Unsupported,
                };
                StateStackApplyLayerReport {
                    index: layer.index,
                    key: layer.key.clone(),
                    role: layer.role,
                    apply_mode: layer.apply_mode,
                    source: layer.source.clone(),
                    target: layer.sql_target.clone(),
                    hook: layer.hook.clone(),
                    status,
                    output: None,
                    artifact_report: None,
                    sql_report: None,
                    hook_status: None,
                    hook_context_path: None,
                    hook_output: None,
                    hook_error: None,
                    error: None,
                }
            })
            .collect();
        Self {
            schema: STATE_STACK_APPLY_SCHEMA.to_owned(),
            schema_version: 1,
            ok: true,
            executed: execute,
            stack_name: lineage.stack_name.clone(),
            environment: lineage.environment,
            lineage_id: lineage.lineage_id.clone(),
            layers,
            warnings: lineage.warnings.clone(),
            written_report_path: None,
            written_history_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStackApplyLayerReport {
    pub index: usize,
    pub key: String,
    pub role: StateLayerRole,
    pub apply_mode: StateLayerApplyMode,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    pub status: StateStackApplyLayerStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_status: Option<StateStackApplyHookStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_context_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateApplyHookContext {
    pub schema: String,
    pub schema_version: u8,
    pub stack_name: String,
    pub environment: String,
    pub lineage_id: String,
    pub layer: StateApplyHookLayerContext,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateApplyHookLayerContext {
    pub index: usize,
    pub key: String,
    pub role: String,
    pub apply_mode: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_report: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_report: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackApplyHookStatus {
    Executed,
    Failed,
}

impl fmt::Display for StateStackApplyHookStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Executed => "executed",
            Self::Failed => "failed",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackApplyLayerStatus {
    WouldExecute,
    WouldStage,
    WouldImport,
    PlannedTask,
    PlannedArtifactStage,
    PlannedSqlImport,
    Executed,
    Staged,
    Imported,
    Skipped,
    Unsupported,
    Failed,
}

impl fmt::Display for StateStackApplyLayerStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::WouldExecute => "would-execute",
            Self::WouldStage => "would-stage",
            Self::WouldImport => "would-import",
            Self::PlannedTask => "planned-task",
            Self::PlannedArtifactStage => "planned-artifact-stage",
            Self::PlannedSqlImport => "planned-sql-import",
            Self::Executed => "executed",
            Self::Staged => "staged",
            Self::Imported => "imported",
            Self::Skipped => "skipped",
            Self::Unsupported => "unsupported",
            Self::Failed => "failed",
        })
    }
}
