use std::collections::BTreeSet;
use std::fmt;

use effigy_artifacts::{ArtifactKind, ArtifactRefError, ArtifactSourceRef};
use serde::{Deserialize, Serialize};

pub const STATE_STACK_SCHEMA: &str = "effigy.state-stack.v1";
pub const STATE_STACK_LINEAGE_SCHEMA: &str = "effigy.state-stack.lineage.v1";

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

    fn from_manifest(manifest: &StateStackManifest) -> Self {
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

pub fn validate_state_stack(
    manifest: &StateStackManifest,
) -> Result<(), StateStackValidationError> {
    if manifest.schema != STATE_STACK_SCHEMA {
        return Err(StateStackValidationError::UnsupportedSchema {
            schema: manifest.schema.clone(),
        });
    }
    if manifest.name.trim().is_empty() {
        return Err(StateStackValidationError::EmptyStackName);
    }
    if manifest.layers.is_empty() {
        return Err(StateStackValidationError::NoLayers);
    }

    let mut seen_keys = BTreeSet::new();
    let mut previous_order = 0;
    for (index, layer) in manifest.layers.iter().enumerate() {
        let key = layer.key.trim();
        if key.is_empty() {
            return Err(StateStackValidationError::EmptyLayerKey { index });
        }
        if !seen_keys.insert(key.to_owned()) {
            return Err(StateStackValidationError::DuplicateLayerKey {
                index,
                key: key.to_owned(),
            });
        }
        let order = layer_role_order(layer.role);
        if order < previous_order {
            return Err(StateStackValidationError::LayerOrder {
                index,
                key: layer.key.clone(),
                role: layer.role,
            });
        }
        previous_order = order;
        validate_environment_policy(manifest.environment, layer, index)?;
        if !matches!(
            layer.role,
            StateLayerRole::Structure
                | StateLayerRole::BaselineSeed
                | StateLayerRole::LegacyImport
                | StateLayerRole::MediaLibrary
                | StateLayerRole::DevOverlay
                | StateLayerRole::UatCapture
                | StateLayerRole::FullCapture
        ) && !matches!(
            layer.apply_mode,
            StateLayerApplyMode::Manual | StateLayerApplyMode::Checkpoint
        ) {
            return Err(StateStackValidationError::DeferredExecutableRole {
                index,
                key: layer.key.clone(),
                role: layer.role,
            });
        }
        layer.artifact_source().map_err(|source| {
            StateStackValidationError::InvalidArtifactSource {
                index,
                key: layer.key.clone(),
                source,
            }
        })?;
    }

    for (index, layer) in manifest.layers.iter().enumerate() {
        for dependency in &layer.depends_on {
            if !seen_keys.contains(dependency) {
                return Err(StateStackValidationError::UnknownDependency {
                    index,
                    key: layer.key.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }

    Ok(())
}

fn validate_environment_policy(
    environment: StateEnvironment,
    layer: &StateStackLayer,
    index: usize,
) -> Result<(), StateStackValidationError> {
    let allowed = match layer.environment_policy {
        StateLayerEnvironmentPolicy::All => true,
        StateLayerEnvironmentPolicy::DevOnly => environment == StateEnvironment::Dev,
        StateLayerEnvironmentPolicy::NonProduction => environment != StateEnvironment::Production,
        StateLayerEnvironmentPolicy::Production => environment == StateEnvironment::Production,
        StateLayerEnvironmentPolicy::CaptureOnly => matches!(
            layer.role,
            StateLayerRole::UatCapture | StateLayerRole::FullCapture
        ),
    };

    if allowed {
        Ok(())
    } else {
        Err(StateStackValidationError::EnvironmentPolicy {
            index,
            key: layer.key.clone(),
            environment,
            policy: layer.environment_policy,
        })
    }
}

fn layer_role_order(role: StateLayerRole) -> usize {
    match role {
        StateLayerRole::Structure => 10,
        StateLayerRole::BaselineSeed => 20,
        StateLayerRole::LegacyImport => 30,
        StateLayerRole::MediaLibrary => 40,
        StateLayerRole::BaseApply => 50,
        StateLayerRole::DevOverlay => 60,
        StateLayerRole::WorkingBaseline => 70,
        StateLayerRole::UatCapture => 80,
        StateLayerRole::LegacyRefresh => 90,
        StateLayerRole::Rebase => 100,
        StateLayerRole::SchemaEvolution => 110,
        StateLayerRole::FullCapture => 120,
    }
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

#[derive(Debug)]
pub enum StateStackParseError {
    Toml(toml::de::Error),
}

impl fmt::Display for StateStackParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => write!(formatter, "failed to parse state stack manifest: {error}"),
        }
    }
}

impl std::error::Error for StateStackParseError {}

#[derive(Debug)]
pub enum StateStackValidationError {
    UnsupportedSchema {
        schema: String,
    },
    EmptyStackName,
    NoLayers,
    EmptyLayerKey {
        index: usize,
    },
    DuplicateLayerKey {
        index: usize,
        key: String,
    },
    LayerOrder {
        index: usize,
        key: String,
        role: StateLayerRole,
    },
    DeferredExecutableRole {
        index: usize,
        key: String,
        role: StateLayerRole,
    },
    EnvironmentPolicy {
        index: usize,
        key: String,
        environment: StateEnvironment,
        policy: StateLayerEnvironmentPolicy,
    },
    InvalidArtifactSource {
        index: usize,
        key: String,
        source: ArtifactRefError,
    },
    UnknownDependency {
        index: usize,
        key: String,
        dependency: String,
    },
}

impl fmt::Display for StateStackValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSchema { schema } => {
                write!(formatter, "unsupported state stack schema `{schema}`")
            }
            Self::EmptyStackName => formatter.write_str("state stack name is empty"),
            Self::NoLayers => formatter.write_str("state stack has no layers"),
            Self::EmptyLayerKey { index } => {
                write!(formatter, "state stack layer {index} has an empty key")
            }
            Self::DuplicateLayerKey { index, key } => {
                write!(formatter, "state stack layer {index} duplicates key `{key}`")
            }
            Self::LayerOrder { index, key, role } => write!(
                formatter,
                "state stack layer {index} `{key}` has out-of-order role `{role:?}`"
            ),
            Self::DeferredExecutableRole { index, key, role } => write!(
                formatter,
                "state stack layer {index} `{key}` uses deferred executable role `{role:?}`"
            ),
            Self::EnvironmentPolicy {
                index,
                key,
                environment,
                policy,
            } => write!(
                formatter,
                "state stack layer {index} `{key}` policy `{policy:?}` does not apply to `{environment:?}`"
            ),
            Self::InvalidArtifactSource { index, key, source } => write!(
                formatter,
                "state stack layer {index} `{key}` has invalid artifact source: {source}"
            ),
            Self::UnknownDependency {
                index,
                key,
                dependency,
            } => write!(
                formatter,
                "state stack layer {index} `{key}` depends on unknown layer `{dependency}`"
            ),
        }
    }
}

impl std::error::Error for StateStackValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_acowtancy_shaped_stack_and_plans_lineage() {
        let manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse manifest");
        let plan = manifest.plan_lineage().expect("plan lineage");

        assert_eq!(plan.schema, STATE_STACK_LINEAGE_SCHEMA);
        assert_eq!(plan.stack_name, "acowtancy-uat");
        assert_eq!(plan.environment, StateEnvironment::Uat);
        assert_eq!(
            plan.layers
                .iter()
                .map(|layer| layer.key.as_str())
                .collect::<Vec<_>>(),
            vec![
                "structure",
                "baseline-seed",
                "legacy-content",
                "dev-users",
                "uat-content-capture",
                "full-system-capture",
            ]
        );
        assert_eq!(plan.artifact_reports.len(), 3);
        assert_eq!(
            plan.artifact_reports
                .iter()
                .map(|report| report.layer_key.as_str())
                .collect::<Vec<_>>(),
            vec!["baseline-seed", "legacy-content", "uat-content-capture"]
        );
    }

    #[test]
    fn lineage_report_is_deterministic() {
        let manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse manifest");
        let report = manifest
            .plan_lineage()
            .expect("plan lineage")
            .report("2026-05-08T12:00:00Z");

        assert_eq!(report.schema, STATE_STACK_LINEAGE_SCHEMA);
        assert_eq!(
            report.lineage_id,
            "acowtancy-uat:Uat:structure+baseline-seed+legacy-content+dev-users+uat-content-capture+full-system-capture"
        );
        assert_eq!(
            report.layers[2].artifact_source.as_deref(),
            Some("oci://ghcr.io/acowtancy/legacy-content:2026-05-08")
        );
        assert_eq!(report.created_at, "2026-05-08T12:00:00Z");
    }

    #[test]
    fn rejects_out_of_order_roles() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.layers.swap(0, 1);

        let error = manifest.plan_lineage().expect_err("reject order");
        assert!(matches!(
            error,
            StateStackValidationError::LayerOrder { .. }
        ));
    }

    #[test]
    fn rejects_dev_only_layer_in_production_stack() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.environment = StateEnvironment::Production;

        let error = manifest.plan_lineage().expect_err("reject environment");
        assert!(matches!(
            error,
            StateStackValidationError::EnvironmentPolicy { .. }
        ));
    }

    #[test]
    fn rejects_ambiguous_oci_source_for_artifact_layer() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.layers[2].source = "ghcr.io/acowtancy/legacy-content:latest".to_owned();

        let error = manifest.plan_lineage().expect_err("reject source");
        assert!(matches!(
            error,
            StateStackValidationError::InvalidArtifactSource { .. }
        ));
    }

    #[test]
    fn accepts_media_library_object_store_layers() {
        let manifest = StateStackManifest::parse_toml(media_fixture()).expect("parse");

        let plan = manifest.plan_lineage().expect("plan");

        assert_eq!(plan.layers[1].key, "legacy-media");
        assert_eq!(plan.layers[1].role, StateLayerRole::MediaLibrary);
        assert_eq!(plan.layers[1].sql_target.as_deref(), Some("media"));
        assert_eq!(
            plan.artifact_reports[0].artifact_kind,
            Some(effigy_artifacts::ArtifactKind::ObjectStore)
        );
    }

    #[test]
    fn requires_deferred_roles_to_be_manual_or_checkpoint() {
        let mut manifest = StateStackManifest::parse_toml(acowtancy_fixture()).expect("parse");
        manifest.layers.insert(
            5,
            StateStackLayer {
                key: "legacy-refresh".to_owned(),
                role: StateLayerRole::LegacyRefresh,
                source: "farmyard:legacy-refresh".to_owned(),
                apply_mode: StateLayerApplyMode::Task,
                environment_policy: StateLayerEnvironmentPolicy::NonProduction,
                depends_on: Vec::new(),
                artifact_kind: None,
                snapshot_identity: None,
                hook: None,
                notes: None,
                sql_target: None,
            },
        );

        let error = manifest.plan_lineage().expect_err("reject deferred role");
        assert!(matches!(
            error,
            StateStackValidationError::DeferredExecutableRole { .. }
        ));
    }

    fn acowtancy_fixture() -> &'static str {
        r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"
hook = "farmyard:db:migrate"

[[layers]]
key = "baseline-seed"
role = "baseline-seed"
source = "./seed/static.sql"
apply_mode = "sql"
environment_policy = "all"
artifact_kind = "sql-dump"

[[layers]]
key = "legacy-content"
role = "legacy-import"
source = "oci://ghcr.io/acowtancy/legacy-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "migrated-base-snapshot"
snapshot_identity = "legacy-db-2026-05-08"
hook = "farmyard:seed-bundle:apply"

[[layers]]
key = "dev-users"
role = "dev-overlay"
source = "farmyard:dev-seed-users"
apply_mode = "task"
environment_policy = "non-production"
hook = "farmyard:dev-seed-users"

[[layers]]
key = "uat-content-capture"
role = "uat-capture"
source = "oci://ghcr.io/acowtancy/uat-content:2026-05-08"
apply_mode = "artifact"
environment_policy = "capture-only"
artifact_kind = "uat-content-snapshot"

[[layers]]
key = "full-system-capture"
role = "full-capture"
source = "farmyard:full-capture"
apply_mode = "checkpoint"
environment_policy = "capture-only"
"#
    }

    fn media_fixture() -> &'static str {
        r#"
schema = "effigy.state-stack.v1"
name = "acowtancy-uat"
environment = "uat"

[[layers]]
key = "structure"
role = "structure"
source = "farmyard:db:migrate"
apply_mode = "task"
environment_policy = "all"

[[layers]]
key = "legacy-media"
role = "media-library"
source = "farmyard/state/legacy/dist/oci/media.oci"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
snapshot_identity = "legacy-media@local"
target = "media"
"#
    }
}
