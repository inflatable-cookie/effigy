use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use effigy_manifest::{
    ManifestTask, ManifestTaskLikeDefinition, ManifestTaskOrReferenceDefinition, ManifestTaskRunIn,
};
use serde::Deserialize;
use toml::Value;

use crate::{
    StateEnvironment, StateLayerApplyMode, StateLayerEnvironmentPolicy, StateLayerRole,
    StateStackLayer, StateStackManifest,
};

#[derive(Debug, Deserialize)]
pub struct StateManifestConfig {
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    default_stack: Option<String>,
    #[serde(default)]
    stacks: BTreeMap<String, StateManifestStackConfig>,
    #[serde(flatten)]
    named_stacks: BTreeMap<String, StateManifestStackConfig>,
}

#[derive(Debug)]
pub struct ResolvedStateStackForApply {
    pub manifest: StateStackManifest,
    pub hooks: BTreeMap<String, ManifestTaskOrReferenceDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateManifestCaptureProfile {
    pub role: String,
    #[serde(alias = "source_environment")]
    pub source_env: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default, alias = "ref")]
    pub destination_ref: Option<String>,
    #[serde(default)]
    pub hook: Option<String>,
    #[serde(default)]
    pub task: Option<ManifestTaskOrReferenceDefinition>,
    #[serde(default)]
    pub push: bool,
}

#[derive(Debug, Clone)]
pub struct StateCaptureRequestDefinition {
    pub profile: Option<String>,
    pub role: Option<String>,
    pub source_env: Option<String>,
    pub key: Option<String>,
    pub source: Option<String>,
    pub destination_ref: Option<String>,
    pub hook: Option<String>,
    pub task: Option<ManifestTaskOrReferenceDefinition>,
    pub yes: bool,
    pub push: bool,
}

impl StateManifestConfig {
    pub fn select_stack_manifest(
        mut self,
        stack: Option<&str>,
    ) -> Result<StateStackManifest, StateManifestConfigError> {
        let selected = self.select_stack_name(stack)?;
        self.stacks
            .remove(&selected)
            .map(StateManifestStackConfig::into_manifest)
            .ok_or_else(|| StateManifestConfigError::UnknownStack {
                stack: selected,
                available: self.available_stack_names(),
            })
    }

    pub fn select_stack_for_apply(
        mut self,
        stack: Option<&str>,
    ) -> Result<ResolvedStateStackForApply, StateManifestConfigError> {
        let selected = self.select_stack_name(stack)?;
        self.stacks
            .remove(&selected)
            .map(StateManifestStackConfig::into_apply)
            .ok_or_else(|| StateManifestConfigError::UnknownStack {
                stack: selected,
                available: self.available_stack_names(),
            })
    }

    pub fn capture_profile(
        mut self,
        stack: &str,
        profile: &str,
    ) -> Result<StateManifestCaptureProfile, StateManifestConfigError> {
        self.merge_named_stacks();
        let stack = self.stacks.remove(stack).ok_or_else(|| {
            StateManifestConfigError::MissingNamedStack {
                stack: stack.to_owned(),
            }
        })?;
        stack.captures.get(profile).cloned().ok_or_else(|| {
            StateManifestConfigError::MissingCaptureProfile {
                stack: stack.name,
                profile: profile.to_owned(),
            }
        })
    }

    fn select_stack_name(
        &mut self,
        stack: Option<&str>,
    ) -> Result<String, StateManifestConfigError> {
        self.merge_named_stacks();
        if let Some(stack) = stack {
            return Ok(stack.to_owned());
        }
        if let Some(default_stack) = self.default.clone().or(self.default_stack.clone()) {
            return Ok(default_stack);
        }
        if self.stacks.len() == 1 {
            return Ok(self.stacks.keys().next().cloned().expect("one stack"));
        }
        if self.stacks.is_empty() {
            return Err(StateManifestConfigError::NoStacks);
        }
        Err(StateManifestConfigError::AmbiguousDefault {
            available: self.available_stack_names(),
        })
    }

    fn merge_named_stacks(&mut self) {
        self.stacks.append(&mut self.named_stacks);
    }

    fn available_stack_names(&self) -> Vec<String> {
        let mut names = self.stacks.keys().cloned().collect::<Vec<_>>();
        names.extend(self.named_stacks.keys().cloned());
        names.sort();
        names
    }
}

pub fn resolve_explicit_manifest_path(
    manifest: &Path,
    invocation_cwd: &Path,
    repo_root: &Path,
) -> PathBuf {
    if manifest.is_absolute() {
        return manifest.to_path_buf();
    }
    let cwd_relative = invocation_cwd.join(manifest);
    if cwd_relative.exists() {
        return cwd_relative;
    }
    repo_root.join(manifest)
}

pub fn load_state_stack_manifest_file(
    manifest: &Path,
    invocation_cwd: &Path,
    repo_root: &Path,
) -> Result<StateStackManifest, StateManifestConfigError> {
    let manifest_path = resolve_explicit_manifest_path(manifest, invocation_cwd, repo_root);
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        StateManifestConfigError::ManifestReadFailed {
            path: manifest_path.display().to_string(),
            error: error.to_string(),
        }
    })?;
    StateStackManifest::parse_toml(&manifest_text)
        .map_err(|error| StateManifestConfigError::ManifestParseFailed(error.to_string()))
}

pub fn parse_state_manifest_config_value(
    state_value: Value,
) -> Result<StateManifestConfig, StateManifestConfigError> {
    state_value.try_into().map_err(|error: toml::de::Error| {
        StateManifestConfigError::ConfigParseFailed(error.to_string())
    })
}

pub fn select_state_stack_manifest(
    state_value: Value,
    stack: Option<&str>,
) -> Result<StateStackManifest, StateManifestConfigError> {
    parse_state_manifest_config_value(state_value)?.select_stack_manifest(stack)
}

pub fn select_state_stack_for_apply(
    state_value: Value,
    stack: Option<&str>,
) -> Result<ResolvedStateStackForApply, StateManifestConfigError> {
    parse_state_manifest_config_value(state_value)?.select_stack_for_apply(stack)
}

pub fn capture_profile_from_state_value(
    state_value: Value,
    stack: &str,
    profile: &str,
) -> Result<StateManifestCaptureProfile, StateManifestConfigError> {
    parse_state_manifest_config_value(state_value)?.capture_profile(stack, profile)
}

pub fn resolve_capture_request(
    stack: Option<&str>,
    manifest: Option<&Path>,
    mut request: StateCaptureRequestDefinition,
    profile_lookup: impl FnOnce(
        &str,
        &str,
    ) -> Result<StateManifestCaptureProfile, StateManifestConfigError>,
) -> Result<StateCaptureRequestDefinition, StateManifestConfigError> {
    let Some(profile_name) = request.profile.clone() else {
        require_capture_fields(&request)?;
        return Ok(request);
    };
    if manifest.is_some() {
        return Err(StateManifestConfigError::CaptureProfileWithStandaloneManifest);
    }
    let stack_name = stack.ok_or(StateManifestConfigError::CaptureProfileMissingStack)?;
    let profile = profile_lookup(stack_name, &profile_name)?;
    if request.role.is_none() {
        request.role = Some(profile.role);
    }
    if request.source_env.is_none() {
        request.source_env = Some(profile.source_env);
    }
    if request.key.is_none() {
        request.key = Some(profile.key.unwrap_or_else(|| profile_name.clone()));
    }
    if request.source.is_none() {
        request.source = profile.source.map(|value| {
            expand_capture_template(
                &value,
                stack_name,
                &profile_name,
                request.key.as_deref().unwrap_or(&profile_name),
            )
        });
    }
    if request.destination_ref.is_none() {
        request.destination_ref = profile.destination_ref.map(|value| {
            expand_capture_template(
                &value,
                stack_name,
                &profile_name,
                request.key.as_deref().unwrap_or(&profile_name),
            )
        });
    }
    if request.hook.is_none() {
        request.hook = profile.hook;
    }
    if request.task.is_none() {
        request.task = profile.task;
    }
    request.push |= profile.push;
    require_capture_fields(&request)?;
    Ok(request)
}

fn require_capture_fields(
    request: &StateCaptureRequestDefinition,
) -> Result<(), StateManifestConfigError> {
    if request.role.is_none() {
        return Err(StateManifestConfigError::MissingCaptureRole);
    }
    if request.source_env.is_none() {
        return Err(StateManifestConfigError::MissingCaptureSourceEnv);
    }
    if request.key.is_none() {
        return Err(StateManifestConfigError::MissingCaptureKey);
    }
    Ok(())
}

fn expand_capture_template(value: &str, stack: &str, profile: &str, key: &str) -> String {
    value
        .replace("{stack}", stack)
        .replace("{profile}", profile)
        .replace("{key}", key)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateManifestStackConfig {
    schema: String,
    name: String,
    environment: StateEnvironment,
    layers: Vec<StateManifestStackLayerConfig>,
    #[serde(default)]
    captures: BTreeMap<String, StateManifestCaptureProfile>,
    #[serde(default)]
    #[allow(dead_code)]
    targets: BTreeMap<String, toml::Value>,
}

impl StateManifestStackConfig {
    fn into_manifest(self) -> StateStackManifest {
        let Self {
            schema,
            name,
            environment,
            layers,
            captures: _,
            targets: _,
        } = self;
        StateStackManifest {
            schema,
            name,
            environment,
            layers: layers
                .into_iter()
                .map(StateManifestStackLayerConfig::into_layer)
                .collect(),
        }
    }

    fn into_apply(self) -> ResolvedStateStackForApply {
        let mut hooks = BTreeMap::new();
        let layers = self
            .layers
            .into_iter()
            .map(|layer| {
                let key = layer.key.clone();
                let (state_layer, hook_definition) = layer.into_layer_and_hook();
                if let Some(definition) = hook_definition {
                    hooks.insert(key, definition);
                }
                state_layer
            })
            .collect();
        ResolvedStateStackForApply {
            manifest: StateStackManifest {
                schema: self.schema,
                name: self.name,
                environment: self.environment,
                layers,
            },
            hooks,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateManifestStackLayerConfig {
    key: String,
    role: StateLayerRole,
    source: String,
    apply_mode: StateLayerApplyMode,
    environment_policy: StateLayerEnvironmentPolicy,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    artifact_kind: Option<effigy_artifacts::ArtifactKind>,
    #[serde(default)]
    snapshot_identity: Option<String>,
    #[serde(default)]
    hook: Option<ManifestTaskOrReferenceDefinition>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default, alias = "target")]
    sql_target: Option<String>,
}

impl StateManifestStackLayerConfig {
    fn into_layer(self) -> StateStackLayer {
        self.into_layer_and_hook().0
    }

    fn into_layer_and_hook(self) -> (StateStackLayer, Option<ManifestTaskOrReferenceDefinition>) {
        let hook_label = self
            .hook
            .as_ref()
            .map(ManifestTaskOrReferenceDefinition::report_name);
        (
            StateStackLayer {
                key: self.key,
                role: self.role,
                source: self.source,
                apply_mode: self.apply_mode,
                environment_policy: self.environment_policy,
                depends_on: self.depends_on,
                artifact_kind: self.artifact_kind,
                snapshot_identity: self.snapshot_identity,
                hook: hook_label,
                notes: self.notes,
                sql_target: self.sql_target,
            },
            self.hook,
        )
    }
}

pub fn state_task_definition_into_manifest_task(
    definition: ManifestTaskOrReferenceDefinition,
) -> Option<ManifestTask> {
    match definition {
        ManifestTaskOrReferenceDefinition::Reference(_) => None,
        ManifestTaskOrReferenceDefinition::TaskLike(ManifestTaskLikeDefinition::Full(task)) => {
            Some(*task)
        }
        ManifestTaskOrReferenceDefinition::TaskLike(definition) => {
            let mut task = definition.into_manifest_task();
            task.run_in.get_or_insert(ManifestTaskRunIn::Host);
            Some(task)
        }
    }
}

#[derive(Debug)]
pub enum StateManifestConfigError {
    NoStacks,
    AmbiguousDefault {
        available: Vec<String>,
    },
    UnknownStack {
        stack: String,
        available: Vec<String>,
    },
    MissingNamedStack {
        stack: String,
    },
    MissingCaptureProfile {
        stack: String,
        profile: String,
    },
    CaptureProfileWithStandaloneManifest,
    CaptureProfileMissingStack,
    MissingCaptureRole,
    MissingCaptureSourceEnv,
    MissingCaptureKey,
    ManifestReadFailed {
        path: String,
        error: String,
    },
    ManifestParseFailed(String),
    ConfigParseFailed(String),
}

impl fmt::Display for StateManifestConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoStacks => {
                formatter.write_str("`[state]` does not define any named state stacks")
            }
            Self::AmbiguousDefault { available } => write!(
                formatter,
                "multiple state stacks are defined; set `state.default` or pass `--stack <NAME>`: {}",
                available.join(", ")
            ),
            Self::UnknownStack { stack, available } => write!(
                formatter,
                "state stack `{stack}` is not defined in `[state]`; available stacks: {}",
                available.join(", ")
            ),
            Self::MissingNamedStack { stack } => {
                write!(formatter, "state stack `{stack}` is not defined in `[state]`")
            }
            Self::MissingCaptureProfile { stack, profile } => write!(
                formatter,
                "state capture profile `{profile}` is not defined in `[state.{stack}.captures]`"
            ),
            Self::CaptureProfileWithStandaloneManifest => formatter.write_str(
                "named capture profiles are loaded from composed `[state]` config and cannot be combined with `--manifest`",
            ),
            Self::CaptureProfileMissingStack => formatter.write_str(
                "named capture profiles require `effigy state capture <stack> <profile>`",
            ),
            Self::MissingCaptureRole => formatter.write_str(
                "`state capture` requires `--role <ROLE>` or a named capture profile",
            ),
            Self::MissingCaptureSourceEnv => formatter.write_str(
                "`state capture` requires `--source-env <ENV>` or a named capture profile",
            ),
            Self::MissingCaptureKey => formatter.write_str(
                "`state capture` requires `--key <LAYER_KEY>` or a named capture profile",
            ),
            Self::ManifestReadFailed { path, error } => {
                write!(formatter, "failed to read state stack manifest {path}: {error}")
            }
            Self::ManifestParseFailed(error) => formatter.write_str(error),
            Self::ConfigParseFailed(error) => {
                write!(formatter, "failed to parse composed `[state]` config: {error}")
            }
        }
    }
}

impl std::error::Error for StateManifestConfigError {}
