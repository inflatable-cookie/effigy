use effigy_artifacts::{ArtifactKind, ArtifactRefError, ArtifactSourceRef};
use serde::{Deserialize, Serialize};

use crate::{
    validate_state_stack, StateStackLineagePlan, StateStackParseError, StateStackValidationError,
};

pub const STATE_STACK_SCHEMA: &str = "effigy.state-stack.v1";
pub const STATE_STACK_LINEAGE_SCHEMA: &str = "effigy.state-stack.lineage.v1";
pub const STATE_STACK_APPLY_SCHEMA: &str = "effigy.state-stack.apply.v1";
pub const STATE_STACK_CAPTURE_SCHEMA: &str = "effigy.state-stack.capture.v1";
pub const STATE_STACK_CAPTURE_SET_SCHEMA: &str = "effigy.state-stack.capture-set.v1";
pub const STATE_STACK_HISTORY_SCHEMA: &str = "effigy.state-stack.history.v1";
pub const STATE_STACK_CAPTURE_CONTEXT_SCHEMA: &str = "effigy.state-stack.capture-context.v1";
pub const STATE_STACK_APPLY_CONTEXT_SCHEMA: &str = "effigy.state-stack.apply-context.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackManifest {
    pub schema: String,
    pub name: String,
    pub environment: StateEnvironment,
    pub layers: Vec<StateStackLayer>,
}

impl StateStackManifest {
    pub fn parse_toml(input: &str) -> Result<Self, StateStackParseError> {
        toml::from_str(input).map_err(StateStackParseError::Toml)
    }

    pub fn plan_lineage(&self) -> Result<StateStackLineagePlan, StateStackValidationError> {
        validate_state_stack(self)?;
        Ok(StateStackLineagePlan::from_manifest(self))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackLayer {
    pub key: String,
    pub role: StateLayerRole,
    pub source: String,
    pub apply_mode: StateLayerApplyMode,
    pub environment_policy: StateLayerEnvironmentPolicy,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub artifact_kind: Option<ArtifactKind>,
    #[serde(default)]
    pub snapshot_identity: Option<String>,
    #[serde(default)]
    pub hook: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default, alias = "target")]
    pub sql_target: Option<String>,
}

impl StateStackLayer {
    pub fn artifact_source(&self) -> Result<Option<ArtifactSourceRef>, ArtifactRefError> {
        match self.apply_mode {
            StateLayerApplyMode::Artifact | StateLayerApplyMode::Sql => {
                ArtifactSourceRef::parse(&self.source).map(Some)
            }
            StateLayerApplyMode::Task
            | StateLayerApplyMode::Manual
            | StateLayerApplyMode::Checkpoint => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateEnvironment {
    Dev,
    Uat,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateLayerRole {
    Structure,
    BaselineSeed,
    LegacyImport,
    MediaLibrary,
    BaseApply,
    DevOverlay,
    WorkingBaseline,
    UatCapture,
    LegacyRefresh,
    Rebase,
    SchemaEvolution,
    FullCapture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateLayerApplyMode {
    Task,
    Artifact,
    Sql,
    Manual,
    Checkpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateLayerEnvironmentPolicy {
    All,
    DevOnly,
    NonProduction,
    Production,
    CaptureOnly,
}

pub fn plain_state_layer_role(role: StateLayerRole) -> String {
    serde_json::to_value(role)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{role:?}"))
}

pub fn plain_state_layer_apply_mode(mode: StateLayerApplyMode) -> String {
    serde_json::to_value(mode)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{mode:?}"))
}

pub fn plain_state_environment(environment: StateEnvironment) -> String {
    serde_json::to_value(environment)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{environment:?}"))
}
