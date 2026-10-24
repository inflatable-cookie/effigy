use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::provider_package::ManifestDeployProviderConfig;
use crate::runner::error::RunnerError;
use crate::runner::manifest::load_task_manifest_with_inspection;

pub(super) fn load_deploy_config(repo_root: &Path) -> Result<ManifestDeployConfig, RunnerError> {
    let loaded =
        load_task_manifest_with_inspection(&repo_root.join(effigy_manifest::TASK_MANIFEST_FILE))?;
    let deploy_value = loaded
        .effective_value
        .get("deploy")
        .cloned()
        .ok_or_else(|| {
            RunnerError::task_invocation(
                "no `[deploy]` section found in the composed manifest".to_owned(),
            )
        })?;
    parse_deploy_config(&deploy_value)
}

pub(super) fn parse_deploy_config(
    deploy_value: &toml::Value,
) -> Result<ManifestDeployConfig, RunnerError> {
    deploy_value.clone().try_into().map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse composed `[deploy]` config: {error}"
        ))
    })
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ManifestDeployConfig {
    #[serde(default)]
    pub(super) providers: BTreeMap<String, ManifestDeployProviderConfig>,
    pub(super) model: Option<toml::Value>,
    #[serde(flatten)]
    pub(super) envs: BTreeMap<String, ManifestDeployEnvConfig>,
}

impl ManifestDeployConfig {
    pub(super) fn declared_environments(&self) -> Vec<String> {
        self.envs.keys().cloned().collect()
    }

    /// Pick the environment a deploy command should describe.
    ///
    /// An explicit `<ENV>` must be declared. Without one, a project with a
    /// single declared environment defaults to it with a notice, and a project
    /// that declares none reports the shared model without environment
    /// overrides. Two or more environments require an explicit `<ENV>`.
    pub(super) fn resolve_environment(
        &self,
        requested: Option<&str>,
    ) -> Result<ResolvedDeployEnvironment, RunnerError> {
        if let Some(name) = requested {
            let config = self.envs.get(name).cloned().ok_or_else(|| {
                RunnerError::task_invocation(undeclared_environment_message(
                    name,
                    &self.declared_environments(),
                ))
            })?;
            return Ok(ResolvedDeployEnvironment {
                name: Some(name.to_owned()),
                config: Some(config),
                notices: Vec::new(),
            });
        }

        let declared = self.declared_environments();
        match declared.as_slice() {
            [] => Ok(ResolvedDeployEnvironment {
                name: None,
                config: None,
                notices: vec![
                    "no `[deploy.<env>]` environment is declared; reporting the shared `[deploy.model]` without environment overrides".to_owned(),
                ],
            }),
            [only] => Ok(ResolvedDeployEnvironment {
                name: Some(only.clone()),
                config: self.envs.get(only).cloned(),
                notices: vec![format!(
                    "no <ENV> given; using the only declared environment `{only}`. Name the environment explicitly in a later release; the single-environment default is available for one release"
                )],
            }),
            many => Err(RunnerError::task_invocation(format!(
                "`deploy model` and `deploy export` require <ENV>; declared environments: {}",
                many.join(", ")
            ))),
        }
    }
}

pub(super) struct ResolvedDeployEnvironment {
    pub(super) name: Option<String>,
    pub(super) config: Option<ManifestDeployEnvConfig>,
    pub(super) notices: Vec<String>,
}

impl ResolvedDeployEnvironment {
    pub(super) fn name_or_unscoped(&self) -> &str {
        self.name.as_deref().unwrap_or("unscoped")
    }

    /// Whether the selected environment describes development rather than a
    /// deployed target. An undeclared environment (the shared model alone) is
    /// treated as development so nothing is suppressed from it.
    pub(super) fn is_development(&self) -> bool {
        let Some(name) = self.name.as_deref() else {
            return true;
        };
        if matches!(name, "dev" | "development" | "local") {
            return true;
        }
        matches!(
            self.config
                .as_ref()
                .and_then(|config| config.state.as_deref()),
            Some("dev" | "development" | "local")
        )
    }

    pub(super) fn state(&self) -> Option<&str> {
        self.config
            .as_ref()
            .and_then(|config| config.state.as_deref())
    }
}

fn undeclared_environment_message(name: &str, declared: &[String]) -> String {
    if declared.is_empty() {
        format!(
            "deploy environment `{name}` is not defined in `[deploy]`; no environments are declared"
        )
    } else {
        format!(
            "deploy environment `{name}` is not defined in `[deploy]`; available environments: {}",
            declared.join(", ")
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ManifestDeployEnvConfig {
    pub(super) provider: ManifestDeployProviderSelection,
    pub(super) state: Option<String>,
    #[serde(default = "default_code_ref")]
    pub(super) code_ref: String,
    #[serde(default)]
    pub(super) release_policy: ReleasePolicy,
    pub(super) provider_project: Option<String>,
    #[serde(default)]
    pub(super) artifact_policy: ArtifactPolicy,
    pub(super) preflight: Option<DeployPreflightConfig>,
    pub(super) hooks: Option<DeployHooksConfig>,
    /// Per-environment deployment-model overrides. Parsed by the model
    /// derivation because the shape is owned by the deployment model.
    pub(super) model: Option<toml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestDeployProviderSelection {
    Name(String),
    Config(ManifestDeployProviderEnvConfig),
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct ManifestDeployProviderEnvConfig {
    pub(super) adapter: String,
    #[serde(flatten)]
    pub(super) config: BTreeMap<String, toml::Value>,
}

impl ManifestDeployProviderSelection {
    pub(super) fn adapter(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Config(config) => config.adapter.as_str(),
        }
    }

    pub(super) fn config_value(&self) -> Value {
        match self {
            Self::Name(name) => json!({ "adapter": name }),
            Self::Config(config) => provider_config_to_json(config),
        }
    }
}

fn provider_config_to_json(config: &ManifestDeployProviderEnvConfig) -> Value {
    let mut value = Map::new();
    value.insert("adapter".to_owned(), json!(config.adapter));
    for (key, item) in &config.config {
        value.insert(key.clone(), toml_value_to_json(item));
    }
    Value::Object(value)
}

fn toml_value_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(value) => json!(value),
        toml::Value::Integer(value) => json!(value),
        toml::Value::Float(value) => json!(value),
        toml::Value::Boolean(value) => json!(value),
        toml::Value::Datetime(value) => json!(value.to_string()),
        toml::Value::Array(values) => Value::Array(values.iter().map(toml_value_to_json).collect()),
        toml::Value::Table(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), toml_value_to_json(value)))
                .collect(),
        ),
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployPreflightConfig {
    #[serde(default)]
    pub(super) require_release_gates: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct DeployHooksConfig {
    pub(super) before_state: Option<String>,
    pub(super) after_state: Option<String>,
    pub(super) after_deploy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ReleasePolicy {
    None,
    #[default]
    Optional,
    Required,
}

impl ReleasePolicy {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Optional => "optional",
            Self::Required => "required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ArtifactPolicy {
    MutableOk,
    #[default]
    DigestPreferred,
    DigestPinned,
}

impl ArtifactPolicy {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::MutableOk => "mutable-ok",
            Self::DigestPreferred => "digest-preferred",
            Self::DigestPinned => "digest-pinned",
        }
    }
}

fn default_code_ref() -> String {
    "branch:main".to_owned()
}
