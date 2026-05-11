use std::collections::BTreeMap;
use std::path::Path;

use toml::Value;

use super::export::{
    infer_underlay_bundle_source, render_shipped_bundle_template_with_inputs,
    render_toml_string_array, underlay_bootstrap_sync_paths, underlay_dir_or_default,
    underlay_isolated_dirs, underlay_optional_dir_step, underlay_optional_docs_qa_steps,
    underlay_optional_docs_step,
};
use super::{
    bundle_shared_root_path, bundle_source_path, bundle_spec_from_descriptor,
    derive_bundle_workspace_subdir, insert_bundle_input_value, optional_bundle_integer,
    optional_bundle_string, parse_bundle_descriptor_source, render_toml_string_list,
    required_bundle_string, BundleSpec, ManifestError,
};

const UNDERLAY_BUNDLE_DESCRIPTOR: &str = include_str!("../../bundles/underlay/bundle.toml");
const UNDERLAY_TEMPLATE: &str = include_str!("../../bundles/underlay/export.toml");

pub(super) fn underlay_spec() -> BundleSpec {
    let path = bundle_source_path("underlay");
    let descriptor = parse_bundle_descriptor_source(&path, UNDERLAY_BUNDLE_DESCRIPTOR)
        .expect("embedded underlay bundle descriptor must parse");
    bundle_spec_from_descriptor(&descriptor)
}

pub(super) fn resolve_underlay_bundle(
    manifest_path: &Path,
    current: &Value,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    let host = required_bundle_string(manifest_path, "underlay", inputs, "host")?;
    let project_name = required_bundle_string(manifest_path, "underlay", inputs, "project_name")?;
    let workspace_subdir =
        required_bundle_string(manifest_path, "underlay", inputs, "workspace_subdir")?;
    let database = required_bundle_string(manifest_path, "underlay", inputs, "database")?;
    let api_port = optional_bundle_integer(inputs, "api_port").unwrap_or(41001);
    let admin_port = optional_bundle_integer(inputs, "admin_port").unwrap_or(41002);
    let front_port = optional_bundle_integer(inputs, "front_port").unwrap_or(41003);
    let system_name =
        optional_bundle_string(inputs, "system_name").unwrap_or_else(|| "dev".to_owned());
    let container_name =
        optional_bundle_string(inputs, "container_name").unwrap_or_else(|| "stack".to_owned());
    let workspace_service_name = optional_bundle_string(inputs, "workspace_service_name")
        .unwrap_or_else(|| "workspace".to_owned());
    let default_workspace =
        optional_bundle_string(inputs, "default_workspace").unwrap_or_else(|| "app".to_owned());
    let underlay_source = infer_underlay_bundle_source(
        current,
        &system_name,
        optional_bundle_string(inputs, "sources.underlay"),
        "underlay",
        "../underlay",
    );
    let poodle_source = infer_underlay_bundle_source(
        current,
        &system_name,
        optional_bundle_string(inputs, "sources.poodle"),
        "poodle",
        "../poodle",
    );
    let bootstrap_sync_paths = underlay_bootstrap_sync_paths(inputs, &underlay_source);
    let bootstrap_sync_command = format!("bootstrap deps sync {}", bootstrap_sync_paths.join(" "));
    let mut render_inputs = inputs.clone();
    render_inputs.insert("host".to_owned(), Value::String(host));
    render_inputs.insert("project_name".to_owned(), Value::String(project_name));
    render_inputs.insert(
        "workspace_subdir".to_owned(),
        Value::String(workspace_subdir),
    );
    render_inputs.insert("database".to_owned(), Value::String(database));
    render_inputs.insert("api_port".to_owned(), Value::Integer(api_port));
    render_inputs.insert("admin_port".to_owned(), Value::Integer(admin_port));
    render_inputs.insert("front_port".to_owned(), Value::Integer(front_port));
    render_inputs.insert("system_name".to_owned(), Value::String(system_name));
    render_inputs.insert("container_name".to_owned(), Value::String(container_name));
    render_inputs.insert(
        "workspace_service_name".to_owned(),
        Value::String(workspace_service_name),
    );
    render_inputs.insert(
        "default_workspace".to_owned(),
        Value::String(default_workspace),
    );
    insert_bundle_input_value(
        &mut render_inputs,
        "sources.underlay",
        Value::String(underlay_source.clone()),
    );
    insert_bundle_input_value(
        &mut render_inputs,
        "sources.poodle",
        Value::String(poodle_source.clone()),
    );
    let rendered = render_shipped_bundle_template_with_inputs(
        manifest_path,
        "underlay",
        &UNDERLAY_TEMPLATE
            .replace(
                "__DATABASES__",
                &render_toml_string_list(inputs, "databases"),
            )
            .replace("__UNDERLAY_SOURCE__", &underlay_source)
            .replace("__POODLE_SOURCE__", &poodle_source)
            .replace("__BOOTSTRAP_SYNC_COMMAND__", &bootstrap_sync_command)
            .replace(
                "__DOCS_HEALTH__",
                &underlay_optional_docs_step(inputs, "health"),
            )
            .replace(
                "__DOCS_VALIDATE__",
                &underlay_optional_docs_step(inputs, "validate"),
            )
            .replace("__DOCS_QA__", &underlay_optional_docs_qa_steps(inputs))
            .replace(
                "__API_DIR__",
                &underlay_dir_or_default(inputs, "dirs.api", "app-api"),
            )
            .replace(
                "__CLIENT_DIR__",
                &underlay_dir_or_default(inputs, "dirs.client", "app-client"),
            )
            .replace(
                "__UI_HEALTH__",
                &underlay_optional_dir_step(inputs, "dirs.ui", "app-ui", "health"),
            )
            .replace(
                "__UI_VALIDATE__",
                &underlay_optional_dir_step(inputs, "dirs.ui", "app-ui", "validate"),
            )
            .replace(
                "__ADMIN_DIR__",
                &underlay_dir_or_default(inputs, "dirs.admin", "app-admin"),
            )
            .replace(
                "__FRONT_DIR__",
                &underlay_dir_or_default(inputs, "dirs.front", "app-front"),
            )
            .replace(
                "__ISOLATED_DIRS__",
                &render_toml_string_array(&underlay_isolated_dirs(inputs)),
            ),
        &render_inputs,
    )?;

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("underlay"),
        error,
    })
}
