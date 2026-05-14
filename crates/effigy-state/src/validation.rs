use std::collections::BTreeSet;
use std::fmt;

use effigy_artifacts::ArtifactRefError;

use crate::{
    model::STATE_STACK_SCHEMA, StateEnvironment, StateLayerApplyMode, StateLayerEnvironmentPolicy,
    StateLayerRole, StateStackLayer, StateStackManifest,
};

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
        StateLayerEnvironmentPolicy::CaptureOnly => {
            matches!(
                layer.role,
                StateLayerRole::UatCapture | StateLayerRole::FullCapture
            )
        }
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
            Self::EnvironmentPolicy { index, key, environment, policy } => write!(
                formatter,
                "state stack layer {index} `{key}` policy `{policy:?}` does not apply to `{environment:?}`"
            ),
            Self::InvalidArtifactSource { index, key, source } => write!(
                formatter,
                "state stack layer {index} `{key}` has invalid artifact source: {source}"
            ),
            Self::UnknownDependency { index, key, dependency } => write!(
                formatter,
                "state stack layer {index} `{key}` depends on unknown layer `{dependency}`"
            ),
        }
    }
}

impl std::error::Error for StateStackValidationError {}
