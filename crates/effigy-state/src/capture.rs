use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use effigy_artifacts::ArtifactKind;
use serde::Serialize;
use serde_json::Value;

use crate::{
    plain_state_layer_role, resolve_repo_relative_path, StateLayerApplyMode,
    StateLayerEnvironmentPolicy, StateLayerRole, StateStackLineageReport,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateCaptureSetReport {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub executed: bool,
    pub stack: String,
    pub key: String,
    pub created_at: String,
    pub profiles: Vec<String>,
    pub captures: Vec<StateCaptureSetEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_history_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateCaptureSetEntry {
    pub profile: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<StateStackCaptureReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStackCaptureReport {
    pub schema: String,
    pub schema_version: u8,
    pub ok: bool,
    pub executed: bool,
    pub stack_name: String,
    pub source_environment: String,
    pub capture_role: StateLayerRole,
    pub capture_mode: StateCaptureMode,
    pub parent_lineage_id: String,
    pub created_at: String,
    pub produced_layers: Vec<StateStackCaptureProducedLayer>,
    pub capture_artifacts: Vec<StateStackCaptureArtifact>,
    pub tasks: Vec<StateStackCaptureTask>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_history_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateCaptureTaskContext {
    pub schema: String,
    pub schema_version: u8,
    pub stack_name: String,
    pub parent_lineage_id: String,
    pub capture_role: String,
    pub capture_mode: String,
    pub source_environment: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateStackCaptureArtifact {
    pub layer_key: String,
    pub operation: StateCaptureArtifactOperation,
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_report: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateCaptureArtifactOperation {
    PlannedCapture,
    CapturedLocal,
    Pushed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackCaptureTask {
    pub name: String,
    pub status: StateStackCaptureTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackCaptureTaskStatus {
    Planned,
    Executed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCapturePlanRequest {
    pub source_environment: String,
    pub key: String,
    pub source: Option<String>,
    pub destination_ref: Option<String>,
    pub hook: Option<String>,
}

impl StateCapturePlanRequest {
    pub fn new(source_environment: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            source_environment: source_environment.into(),
            key: key.into(),
            source: None,
            destination_ref: None,
            hook: None,
        }
    }

    pub fn source(mut self, source: Option<String>) -> Self {
        self.source = source;
        self
    }

    pub fn destination_ref(mut self, destination_ref: Option<String>) -> Self {
        self.destination_ref = destination_ref;
        self
    }

    pub fn hook(mut self, hook: Option<String>) -> Self {
        self.hook = hook;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStackCaptureProducedLayer {
    pub key: String,
    pub role: StateLayerRole,
    pub apply_mode: StateLayerApplyMode,
    pub environment_policy: StateLayerEnvironmentPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<ArtifactKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_identity: Option<String>,
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
}

pub fn capture_produced_layer(
    lineage: &StateStackLineageReport,
    capture_role: StateLayerRole,
    request: &StateCapturePlanRequest,
) -> Result<StateStackCaptureProducedLayer, StateCapturePlanningError> {
    let capture_mode = StateCaptureMode::from_role(capture_role)
        .ok_or(StateCapturePlanningError::UnsupportedCaptureRole { role: capture_role })?;
    Ok(StateStackCaptureProducedLayer {
        key: request.key.clone(),
        role: capture_role,
        apply_mode: StateLayerApplyMode::Artifact,
        environment_policy: capture_mode.environment_policy(),
        artifact_kind: Some(ArtifactKind::AppSpecific),
        source_ref: request.destination_ref.clone(),
        snapshot_identity: Some(format!(
            "{}@planned",
            capture_mode.snapshot_identity_prefix()
        )),
        depends_on: lineage
            .layers
            .last()
            .map(|layer| vec![layer.key.clone()])
            .unwrap_or_default(),
        hook: request.hook.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateCaptureMode {
    UatOverlay,
    FullSnapshot,
}

impl StateCaptureMode {
    pub fn from_role(role: StateLayerRole) -> Option<Self> {
        match role {
            StateLayerRole::UatCapture => Some(Self::UatOverlay),
            StateLayerRole::FullCapture => Some(Self::FullSnapshot),
            _ => None,
        }
    }

    pub fn environment_policy(self) -> StateLayerEnvironmentPolicy {
        match self {
            Self::UatOverlay => StateLayerEnvironmentPolicy::NonProduction,
            Self::FullSnapshot => StateLayerEnvironmentPolicy::CaptureOnly,
        }
    }

    pub fn snapshot_identity_prefix(self) -> &'static str {
        match self {
            Self::UatOverlay => "uat-authored-content",
            Self::FullSnapshot => "full-system-capture",
        }
    }
}

pub fn parse_capture_role(value: &str) -> Option<StateLayerRole> {
    match value {
        "uat-capture" => Some(StateLayerRole::UatCapture),
        "full-capture" => Some(StateLayerRole::FullCapture),
        _ => None,
    }
}

impl fmt::Display for StateCaptureMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UatOverlay => "uat-overlay",
            Self::FullSnapshot => "full-snapshot",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateCapturePlanningError {
    UnsupportedCaptureRole { role: StateLayerRole },
}

impl fmt::Display for StateCapturePlanningError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCaptureRole { role } => {
                write!(formatter, "unsupported state capture role `{role:?}`")
            }
        }
    }
}

impl std::error::Error for StateCapturePlanningError {}

pub fn state_capture_task_environment(
    repo_root: &Path,
    lineage: &StateStackLineageReport,
    source_environment: &str,
    key: &str,
    source: Option<&str>,
    destination_ref: Option<&str>,
    capture_role: StateLayerRole,
    capture_mode: StateCaptureMode,
    context_path: Option<&str>,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "EFFIGY_STATE_CAPTURE_SCHEMA".to_owned(),
        "effigy.state-stack.capture.v1".to_owned(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_STACK".to_owned(),
        lineage.stack_name.clone(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_PARENT_LINEAGE_ID".to_owned(),
        lineage.lineage_id.clone(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_ROLE".to_owned(),
        plain_state_layer_role(capture_role),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_MODE".to_owned(),
        capture_mode.to_string(),
    );
    env.insert(
        "EFFIGY_STATE_CAPTURE_SOURCE_ENV".to_owned(),
        source_environment.to_owned(),
    );
    env.insert("EFFIGY_STATE_CAPTURE_KEY".to_owned(), key.to_owned());
    if let Some(source) = source {
        env.insert(
            "EFFIGY_STATE_CAPTURE_SOURCE".to_owned(),
            resolve_repo_relative_path(repo_root, source),
        );
    }
    if let Some(destination) = destination_ref {
        env.insert(
            "EFFIGY_STATE_CAPTURE_DESTINATION_REF".to_owned(),
            destination.to_owned(),
        );
    }
    if let Some(context_path) = context_path {
        env.insert(
            "EFFIGY_STATE_CAPTURE_CONTEXT".to_owned(),
            context_path.to_owned(),
        );
    }
    env
}
