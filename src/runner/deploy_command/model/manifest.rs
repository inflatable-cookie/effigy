use std::collections::BTreeMap;

use serde::Deserialize;

use super::DeployWarning;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelConfig {
    pub(super) app: ManifestDeployModelApp,
    #[serde(default)]
    pub(super) services: Vec<ManifestDeployModelService>,
    #[serde(default)]
    pub(super) backing_services: Vec<ManifestDeployModelBackingService>,
    #[serde(default)]
    pub(super) domains: Vec<ManifestDeployModelDomain>,
    #[serde(default)]
    pub(super) secrets: Vec<ManifestDeployModelSecret>,
    #[serde(default)]
    pub(super) warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelApp {
    pub(super) project_name: String,
    #[serde(default)]
    pub(super) bundle: Option<String>,
    #[serde(default)]
    pub(super) source_root: Option<String>,
    #[serde(default)]
    pub(super) notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelService {
    pub(super) name: String,
    pub(super) role: String,
    pub(super) runtime: String,
    pub(super) source_root: String,
    #[serde(default)]
    pub(super) build: Option<ManifestDeployModelTaskStep>,
    #[serde(default)]
    pub(super) start: Option<ManifestDeployModelTaskStep>,
    #[serde(default)]
    pub(super) release: Option<ManifestDeployModelTaskStep>,
    #[serde(default)]
    pub(super) health: Option<ManifestDeployModelHealth>,
    #[serde(default)]
    pub(super) output: Option<ManifestDeployModelOutput>,
    #[serde(default)]
    pub(super) port: Option<u16>,
    #[serde(default)]
    pub(super) domains: Vec<String>,
    #[serde(default)]
    pub(super) env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub(super) secret_refs: Option<Vec<String>>,
    #[serde(default)]
    pub(super) volumes: Option<Vec<String>>,
    #[serde(default)]
    pub(super) warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelTaskStep {
    pub(super) task: String,
    #[serde(default)]
    pub(super) optional: Option<bool>,
    #[serde(default)]
    pub(super) omit_service_if_missing: Option<bool>,
    #[serde(default)]
    pub(super) warn_if_missing: Option<bool>,
    #[serde(default)]
    pub(super) warning_code: Option<String>,
    #[serde(default)]
    pub(super) warning_target: Option<String>,
    #[serde(default)]
    pub(super) warning_message: Option<String>,
    #[serde(default)]
    pub(super) warning_severity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelHealth {
    pub(super) kind: String,
    pub(super) path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelOutput {
    pub(super) kind: String,
    pub(super) path: String,
    #[serde(default)]
    pub(super) fallback: Option<String>,
    #[serde(default)]
    pub(super) detect_static_fallback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelBackingService {
    pub(super) name: String,
    pub(super) kind: String,
    pub(super) mode: String,
    #[serde(default = "default_true")]
    pub(super) required: bool,
    #[serde(default)]
    pub(super) consumers: Vec<String>,
    #[serde(default)]
    pub(super) warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelDomain {
    pub(super) host: String,
    pub(super) service: String,
    pub(super) tls: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelSecret {
    pub(super) name: String,
    #[serde(default)]
    pub(super) services: Vec<String>,
    #[serde(default = "default_true")]
    pub(super) required: bool,
    pub(super) source: String,
    #[serde(default)]
    pub(super) notes: Option<String>,
}

pub(in crate::runner::deploy_command) fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelOverride {
    #[serde(default)]
    pub(super) app: Option<ManifestDeployModelAppOverride>,
    #[serde(default)]
    pub(super) services: Vec<ManifestDeployModelServiceOverride>,
    #[serde(default)]
    pub(super) backing_services: Vec<ManifestDeployModelBackingServiceOverride>,
    #[serde(default)]
    pub(super) domains: Option<Vec<ManifestDeployModelDomain>>,
    #[serde(default)]
    pub(super) secrets: Vec<ManifestDeployModelSecretOverride>,
    #[serde(default)]
    pub(super) warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelAppOverride {
    #[serde(default)]
    pub(super) project_name: Option<String>,
    #[serde(default)]
    pub(super) bundle: Option<String>,
    #[serde(default)]
    pub(super) source_root: Option<String>,
    #[serde(default)]
    pub(super) notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelServiceOverride {
    pub(super) name: String,
    #[serde(default)]
    pub(super) role: Option<String>,
    #[serde(default)]
    pub(super) runtime: Option<String>,
    #[serde(default)]
    pub(super) source_root: Option<String>,
    #[serde(default)]
    pub(super) build: Option<ManifestDeployModelTaskStepOverride>,
    #[serde(default)]
    pub(super) start: Option<ManifestDeployModelTaskStepOverride>,
    #[serde(default)]
    pub(super) release: Option<ManifestDeployModelTaskStepOverride>,
    #[serde(default)]
    pub(super) health: Option<ManifestDeployModelHealthOverride>,
    #[serde(default)]
    pub(super) output: Option<ManifestDeployModelOutputOverride>,
    #[serde(default)]
    pub(super) port: Option<u16>,
    #[serde(default)]
    pub(super) domains: Option<Vec<String>>,
    #[serde(default)]
    pub(super) env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub(super) env_unset: Option<Vec<String>>,
    #[serde(default)]
    pub(super) secret_refs: Option<Vec<String>>,
    #[serde(default)]
    pub(super) volumes: Option<Vec<String>>,
    #[serde(default)]
    pub(super) warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelTaskStepOverride {
    #[serde(default)]
    pub(super) task: Option<String>,
    #[serde(default)]
    pub(super) optional: Option<bool>,
    #[serde(default)]
    pub(super) omit_service_if_missing: Option<bool>,
    #[serde(default)]
    pub(super) warn_if_missing: Option<bool>,
    #[serde(default)]
    pub(super) warning_code: Option<String>,
    #[serde(default)]
    pub(super) warning_target: Option<String>,
    #[serde(default)]
    pub(super) warning_message: Option<String>,
    #[serde(default)]
    pub(super) warning_severity: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelHealthOverride {
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelOutputOverride {
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) path: Option<String>,
    #[serde(default)]
    pub(super) fallback: Option<String>,
    #[serde(default)]
    pub(super) detect_static_fallback: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelBackingServiceOverride {
    pub(super) name: String,
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) mode: Option<String>,
    #[serde(default)]
    pub(super) required: Option<bool>,
    #[serde(default)]
    pub(super) consumers: Option<Vec<String>>,
    #[serde(default)]
    pub(super) warnings: Option<Vec<DeployWarning>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDeployModelSecretOverride {
    pub(super) name: String,
    #[serde(default)]
    pub(super) services: Option<Vec<String>>,
    #[serde(default)]
    pub(super) required: Option<bool>,
    #[serde(default)]
    pub(super) source: Option<String>,
    #[serde(default)]
    pub(super) notes: Option<String>,
}
