//! The config command's rendering: which request renders which payload, and
//! the schema/reference dispatch. Each payload's own rendering lives with its
//! owner: [`payload`] holds the envelope, [`inspect`] the manifest composition
//! view, [`user`] the user-global config view.

use std::path::Path;

use crate::config::reference::{render_config_reference, style_schema_comments};
use crate::config::request::{ConfigRequest, ConfigSchemaTarget};
use crate::config::schema::{
    render_builtin_config_schema, render_builtin_config_schema_bundle_target,
    render_builtin_config_schema_minimal, render_builtin_config_schema_target,
    render_builtin_config_schema_test_target,
};
use crate::render_builtin_help_text;
use crate::response::builtin_output_color_enabled;
use crate::BuiltinError;

mod inspect;
mod payload;
mod user;

use inspect::render_inspect_payload;
use payload::{render_config_payload, ConfigPayload};
use user::{
    render_user_get_payload, render_user_inspect_payload, render_user_path_payload,
    render_user_update_payload,
};

pub(super) fn render_config_request(
    request: ConfigRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    if request.user_path {
        return render_user_path_payload(request);
    }
    if let Some(key) = request.user_get {
        return render_user_get_payload(request, key);
    }
    if request.user_inspect {
        return render_user_inspect_payload(request);
    }
    if request.set_container_backend.is_some()
        || request.set_container_profile.is_some()
        || request.set_container_profile_disk_gib.is_some()
        || request.unset_container_backend
        || request.unset_container_profile
        || request.unset_container_profile_disk_gib
    {
        return render_user_update_payload(request);
    }
    if request.inspect {
        return render_inspect_payload(request, target_root);
    }
    if request.schema {
        return render_schema_payload(request);
    }
    render_reference_payload(request.output_json)
}

pub(super) fn render_config_help_payload(output_json: bool) -> Result<String, BuiltinError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_builtin_help_text("config", rendered, output_json)
}

fn render_schema_payload(request: ConfigRequest) -> Result<Option<String>, BuiltinError> {
    let color_enabled = builtin_output_color_enabled(request.output_json);
    let target = request.target;
    let runner = request.runner;

    let rendered = match target {
        Some(ConfigSchemaTarget::Bundle) => {
            render_builtin_config_schema_bundle_target(request.minimal, None, &[])
        }
        Some(ConfigSchemaTarget::Test) => {
            render_builtin_config_schema_test_target(request.minimal, runner)
        }
        Some(target) => render_builtin_config_schema_target(target, request.minimal),
        None if request.minimal => render_builtin_config_schema_minimal(),
        None => render_builtin_config_schema(),
    };

    let text = style_schema_comments(rendered, color_enabled);
    render_config_payload(
        request.output_json,
        ConfigPayload::schema(request.minimal, target, None, runner, text),
    )
}

fn render_reference_payload(output_json: bool) -> Result<Option<String>, BuiltinError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_config_payload(output_json, ConfigPayload::reference(rendered))
}
