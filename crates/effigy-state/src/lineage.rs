use effigy_artifacts::ArtifactKind;
use serde::{Deserialize, Serialize};

use crate::{
    model::STATE_STACK_LINEAGE_SCHEMA, StateEnvironment, StateLayerApplyMode,
    StateLayerEnvironmentPolicy, StateLayerRole, StateStackManifest,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateStackLineagePlan {
    pub schema: String,
    pub lineage_id: String,
    pub stack_name: String,
    pub environment: StateEnvironment,
    pub layers: Vec<StateStackLineageLayer>,
    pub artifact_reports: Vec<StateStackArtifactReportRef>,
    pub warnings: Vec<String>,
}

impl StateStackLineagePlan {
    pub fn report(self, created_at: impl Into<String>) -> StateStackLineageReport {
        StateStackLineageReport {
            schema: self.schema,
            lineage_id: self.lineage_id,
            stack_name: self.stack_name,
            environment: self.environment,
            created_at: created_at.into(),
            layers: self.layers,
            artifact_reports: self.artifact_reports,
            warnings: self.warnings,
            written_report_path: None,
            written_history_path: None,
        }
    }

    pub(crate) fn from_manifest(manifest: &StateStackManifest) -> Self {
        let mut artifact_reports = Vec::new();
        let mut layers = Vec::with_capacity(manifest.layers.len());
        for (index, layer) in manifest.layers.iter().enumerate() {
            let artifact_source = layer
                .artifact_source()
                .ok()
                .flatten()
                .map(|source| source.display_ref());
            if let Some(source_ref) = artifact_source.as_ref() {
                artifact_reports.push(StateStackArtifactReportRef {
                    layer_key: layer.key.clone(),
                    source_ref: source_ref.clone(),
                    artifact_kind: layer.artifact_kind,
                    operation: StateStackArtifactOperation::PlannedResolve,
                });
            }
            layers.push(StateStackLineageLayer {
                index,
                key: layer.key.clone(),
                role: layer.role,
                apply_mode: layer.apply_mode,
                environment_policy: layer.environment_policy,
                source: layer.source.clone(),
                artifact_source,
                hook: layer.hook.clone(),
                snapshot_identity: layer.snapshot_identity.clone(),
                sql_target: layer.sql_target.clone(),
            });
        }

        Self {
            schema: STATE_STACK_LINEAGE_SCHEMA.to_owned(),
            lineage_id: lineage_id(manifest),
            stack_name: manifest.name.clone(),
            environment: manifest.environment,
            layers,
            artifact_reports,
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackLineageReport {
    pub schema: String,
    pub lineage_id: String,
    pub stack_name: String,
    pub environment: StateEnvironment,
    pub created_at: String,
    pub layers: Vec<StateStackLineageLayer>,
    pub artifact_reports: Vec<StateStackArtifactReportRef>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_report_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_history_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackLineageLayer {
    pub index: usize,
    pub key: String,
    pub role: StateLayerRole,
    pub apply_mode: StateLayerApplyMode,
    pub environment_policy: StateLayerEnvironmentPolicy,
    pub source: String,
    pub artifact_source: Option<String>,
    pub hook: Option<String>,
    pub snapshot_identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateStackArtifactReportRef {
    pub layer_key: String,
    pub source_ref: String,
    pub artifact_kind: Option<ArtifactKind>,
    pub operation: StateStackArtifactOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StateStackArtifactOperation {
    PlannedResolve,
}

fn lineage_id(manifest: &StateStackManifest) -> String {
    let keys = manifest
        .layers
        .iter()
        .map(|layer| layer.key.as_str())
        .collect::<Vec<_>>()
        .join("+");
    format!("{}:{:?}:{}", manifest.name, manifest.environment, keys)
}
