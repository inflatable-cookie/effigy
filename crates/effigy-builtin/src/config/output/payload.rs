//! The `effigy.config.v1` envelope: one payload per config mode.

use serde_json::json;

use crate::config::output::inspect::ConfigInspect;
use crate::config::output::user;
use crate::config::request::{ConfigSchemaTarget, ConfigTestRunner};
use crate::response::render_optional_text_with_schema_text_fields_lazy;
use crate::BuiltinError;
use effigy_manifest::UserConfig;

/// Which config view a payload reports. The mode decides the text and the
/// field set the payload may carry, so a rendering cannot claim a mode it did
/// not render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigPayloadMode {
    Reference,
    Schema,
    Inspect,
    UserPath,
    UserInspect,
    UserGet,
    UserUpdate,
}

impl ConfigPayloadMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Schema => "schema",
            Self::Inspect => "inspect",
            Self::UserPath => "user_path",
            Self::UserInspect => "user_inspect",
            Self::UserGet => "user_get",
            Self::UserUpdate => "user_update",
        }
    }
}

pub(super) fn render_config_payload(
    output_json: bool,
    payload: ConfigPayload,
) -> Result<Option<String>, BuiltinError> {
    let mode = payload.mode.as_str();
    let minimal = payload.minimal;
    let target = payload.target.map(ConfigSchemaTarget::as_str);
    let bundle = payload.bundle;
    let bundle_inputs = payload.bundle_inputs;
    let runner = payload.runner.map(ConfigTestRunner::as_str);
    let manifest_path = payload.manifest_path;
    let evaluation_order = payload.evaluation_order;
    let include_graph = payload.include_graph;
    let overridden_paths = payload.overridden_paths;
    let value_sources = payload.value_sources;
    let effective_manifest = payload.effective_manifest;
    let selected_path = payload.selected_path;
    let selected_value = payload.selected_value;
    let user_config_path = payload.user_config_path;
    let user_config = payload.user_config;
    let user_config_removed = payload.user_config_removed;
    let user_config_key = payload.user_config_key;
    let user_config_value = payload.user_config_value;
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.config.v1",
        move || payload.text,
        move || {
            json!({
                "mode": mode,
                "minimal": minimal,
                "target": target,
                "bundle": bundle,
                "bundle_inputs": bundle_inputs,
                "runner": runner,
                "manifest_path": manifest_path,
                "evaluation_order": evaluation_order,
                "include_graph": include_graph,
                "overridden_paths": overridden_paths,
                "value_sources": value_sources,
                "effective_manifest": effective_manifest,
                "selected_path": selected_path,
                "selected_value": selected_value,
                "user_config_path": user_config_path,
                "user_config": user_config,
                "user_config_removed": user_config_removed,
                "user_config_key": user_config_key,
                "user_config_value": user_config_value,
            })
        },
    )
}

/// One config payload. Every mode uses the same envelope shape (absent fields
/// are null), and each constructor fills only the fields its mode owns.
pub(super) struct ConfigPayload {
    mode: ConfigPayloadMode,
    minimal: bool,
    target: Option<ConfigSchemaTarget>,
    bundle: Option<String>,
    /// Inspect mode only: the active bundle, its declared inputs and where
    /// each effective value came from.
    bundle_inputs: Option<serde_json::Value>,
    runner: Option<ConfigTestRunner>,
    manifest_path: Option<String>,
    evaluation_order: Option<Vec<String>>,
    include_graph: Option<Vec<serde_json::Value>>,
    overridden_paths: Option<Vec<serde_json::Value>>,
    value_sources: Option<Vec<serde_json::Value>>,
    effective_manifest: Option<String>,
    selected_path: Option<String>,
    selected_value: Option<serde_json::Value>,
    user_config_path: Option<String>,
    user_config: Option<serde_json::Value>,
    user_config_removed: bool,
    user_config_key: Option<String>,
    user_config_value: Option<String>,
    text: String,
}

impl ConfigPayload {
    fn new(mode: ConfigPayloadMode, text: String) -> Self {
        Self {
            mode,
            minimal: false,
            target: None,
            bundle: None,
            bundle_inputs: None,
            runner: None,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
            selected_path: None,
            selected_value: None,
            user_config_path: None,
            user_config: None,
            user_config_removed: false,
            user_config_key: None,
            user_config_value: None,
            text,
        }
    }

    pub(super) fn reference(text: String) -> Self {
        Self::new(ConfigPayloadMode::Reference, text)
    }

    pub(super) fn schema(
        minimal: bool,
        target: Option<ConfigSchemaTarget>,
        bundle: Option<String>,
        runner: Option<ConfigTestRunner>,
        text: String,
    ) -> Self {
        let mut payload = Self::new(ConfigPayloadMode::Schema, text);
        payload.minimal = minimal;
        payload.target = target;
        payload.bundle = bundle;
        payload.runner = runner;
        payload
    }

    pub(super) fn inspect(inspect: ConfigInspect) -> Self {
        let ConfigInspect {
            text,
            manifest_path,
            evaluation_order,
            include_graph,
            overridden_paths,
            value_sources,
            effective_manifest,
            bundle_inputs,
            selected_path,
            selected_value,
        } = inspect;
        let mut payload = Self::new(ConfigPayloadMode::Inspect, text);
        payload.manifest_path = Some(manifest_path);
        payload.evaluation_order = Some(evaluation_order);
        payload.include_graph = Some(include_graph);
        payload.overridden_paths = Some(overridden_paths);
        payload.value_sources = Some(value_sources);
        payload.effective_manifest = Some(effective_manifest);
        payload.bundle_inputs = bundle_inputs;
        payload.selected_path = selected_path;
        payload.selected_value = selected_value;
        payload
    }

    pub(super) fn user_path(text: String, user_config_path: String) -> Self {
        let mut payload = Self::new(ConfigPayloadMode::UserPath, text);
        payload.user_config_path = Some(user_config_path);
        payload
    }

    pub(super) fn user_inspect(
        text: String,
        user_config_path: String,
        user_config: &UserConfig,
    ) -> Self {
        let mut payload = Self::new(ConfigPayloadMode::UserInspect, text);
        payload.user_config_path = Some(user_config_path);
        payload.user_config = Some(user::serialize_user_config(user_config));
        payload
    }

    pub(super) fn user_get(
        text: String,
        user_config_path: String,
        user_config: &UserConfig,
        user_config_key: String,
        user_config_value: String,
    ) -> Self {
        let mut payload = Self::new(ConfigPayloadMode::UserGet, text);
        payload.user_config_path = Some(user_config_path);
        payload.user_config = Some(user::serialize_user_config(user_config));
        payload.user_config_key = Some(user_config_key);
        payload.user_config_value = Some(user_config_value);
        payload
    }

    pub(super) fn user_update(
        text: String,
        user_config_path: String,
        user_config: &UserConfig,
        user_config_removed: bool,
    ) -> Self {
        let mut payload = Self::new(ConfigPayloadMode::UserUpdate, text);
        payload.user_config_path = Some(user_config_path);
        payload.user_config = Some(user::serialize_user_config(user_config));
        payload.user_config_removed = user_config_removed;
        payload
    }
}
