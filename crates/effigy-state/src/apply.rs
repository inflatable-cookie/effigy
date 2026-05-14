use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use serde_json::Value;

use crate::{
    plain_state_environment, plain_state_layer_apply_mode, plain_state_layer_role,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateApplyPlanningError {
    UnknownSkipLayers { layers: Vec<String> },
}

impl fmt::Display for StateApplyPlanningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSkipLayers { layers } => write!(
                formatter,
                "state apply skip layer(s) not found: {}",
                layers.join(", ")
            ),
        }
    }
}

impl std::error::Error for StateApplyPlanningError {}

pub fn mark_skipped_apply_layers(
    report: &mut StateStackApplyReport,
    skip_layers: &[String],
) -> Result<(), StateApplyPlanningError> {
    if skip_layers.is_empty() {
        return Ok(());
    }

    let known_layers = report
        .layers
        .iter()
        .map(|layer| layer.key.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut unknown_layers = skip_layers
        .iter()
        .filter(|layer| !known_layers.contains(layer.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    unknown_layers.sort();
    unknown_layers.dedup();
    if !unknown_layers.is_empty() {
        return Err(StateApplyPlanningError::UnknownSkipLayers {
            layers: unknown_layers,
        });
    }

    let skip_layers = skip_layers
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    for layer in &mut report.layers {
        if skip_layers.contains(layer.key.as_str()) {
            layer.status = StateStackApplyLayerStatus::Skipped;
        }
    }

    Ok(())
}

pub fn state_apply_hook_environment(
    stack_name: &str,
    environment: StateEnvironment,
    lineage_id: &str,
    layer: &StateStackApplyLayerReport,
    context_path: &str,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "EFFIGY_STATE_APPLY_SCHEMA".to_owned(),
        STATE_STACK_APPLY_SCHEMA.to_owned(),
    );
    env.insert("EFFIGY_STATE_APPLY_STACK".to_owned(), stack_name.to_owned());
    env.insert(
        "EFFIGY_STATE_APPLY_ENVIRONMENT".to_owned(),
        plain_state_environment(environment),
    );
    env.insert(
        "EFFIGY_STATE_APPLY_LINEAGE_ID".to_owned(),
        lineage_id.to_owned(),
    );
    env.insert("EFFIGY_STATE_APPLY_LAYER_KEY".to_owned(), layer.key.clone());
    env.insert(
        "EFFIGY_STATE_APPLY_LAYER_ROLE".to_owned(),
        plain_state_layer_role(layer.role),
    );
    env.insert(
        "EFFIGY_STATE_APPLY_LAYER_MODE".to_owned(),
        plain_state_layer_apply_mode(layer.apply_mode),
    );
    env.insert(
        "EFFIGY_STATE_APPLY_LAYER_SOURCE".to_owned(),
        layer.source.clone(),
    );
    if let Some(target) = layer.target.as_ref() {
        env.insert("EFFIGY_STATE_APPLY_LAYER_TARGET".to_owned(), target.clone());
    }
    if let Some(hook) = layer.hook.as_ref() {
        env.insert("EFFIGY_STATE_APPLY_HOOK".to_owned(), hook.clone());
    }
    if let Some(artifact_report) = layer.artifact_report.as_ref() {
        if let Some(digest) = artifact_report
            .pointer("/destination/digest")
            .and_then(Value::as_str)
        {
            env.insert("EFFIGY_STATE_APPLY_DIGEST".to_owned(), digest.to_owned());
        }
    }
    env.insert(
        "EFFIGY_STATE_APPLY_CONTEXT".to_owned(),
        context_path.to_owned(),
    );
    env
}
