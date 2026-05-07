use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml::Value;

use super::super::render_builtin_help_text;
use super::super::response::{
    builtin_output_color_enabled, render_optional_text_with_schema_text_fields_lazy,
};
use super::reference::{render_config_reference, style_schema_comments};
use super::request::{ConfigRequest, ConfigSchemaTarget, ConfigTestRunner, UserConfigKey};
use super::schema::{
    render_builtin_config_schema, render_builtin_config_schema_bundle_target,
    render_builtin_config_schema_minimal, render_builtin_config_schema_target,
    render_builtin_config_schema_test_target,
};
use crate::BuiltinError;
use effigy_manifest::{
    get_bundle, list_bundle_default_paths, load_task_manifest_with_inspection, load_user_config,
    save_user_config, user_config_path, ManifestCompositionEdge, ManifestCompositionOverride,
    ManifestCompositionValueSource, UserConfig, UserContainerBackendPreference,
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
        || request.unset_container_backend
        || request.unset_container_profile
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

fn render_user_path_payload(request: ConfigRequest) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let text = format!("{}\n", path.display());
    render_config_payload(
        request.output_json,
        ConfigPayload::user_path(text, path.display().to_string()),
    )
}

fn render_user_get_payload(
    request: ConfigRequest,
    key: UserConfigKey,
) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let config = load_user_config()?;
    let value = render_user_config_key_value(&config, key).ok_or_else(|| {
        BuiltinError::task_invocation(format!(
            "user-global config key `{}` is not set",
            key.as_str()
        ))
    })?;
    let text = format!("{value}\n");
    render_config_payload(
        request.output_json,
        ConfigPayload::user_get(
            text,
            path.display().to_string(),
            &config,
            key.as_str().to_owned(),
            value,
        ),
    )
}

fn render_user_inspect_payload(request: ConfigRequest) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let config = load_user_config()?;
    let text = render_user_config_text(&path, &config, false)?;
    render_config_payload(
        request.output_json,
        ConfigPayload::user_inspect(text, path.display().to_string(), &config),
    )
}

fn render_user_update_payload(request: ConfigRequest) -> Result<Option<String>, BuiltinError> {
    let path = user_config_path().ok_or_else(|| {
        BuiltinError::task_invocation("HOME is not set; cannot resolve user-global config path")
    })?;
    let mut config = load_user_config()?;

    if let Some(backend) = request.set_container_backend {
        config.containers.backend = Some(backend);
    }
    if request.unset_container_backend {
        config.containers.backend = None;
    }
    if let Some(profile) = request.set_container_profile.as_ref() {
        config.containers.profile = Some(profile.clone());
    }
    if request.unset_container_profile {
        config.containers.profile = None;
    }

    let removed_file = if config.is_empty() {
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|error| BuiltinError::task_invocation_failed_write(&path, error))?;
        }
        true
    } else {
        let written_path = save_user_config(&config)?;
        debug_assert_eq!(written_path, path);
        false
    };

    let text = render_user_config_text(&path, &config, removed_file)?;
    render_config_payload(
        request.output_json,
        ConfigPayload::user_update(text, path.display().to_string(), &config, removed_file),
    )
}

pub(super) fn render_config_help_payload(output_json: bool) -> Result<String, BuiltinError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_builtin_help_text("config", rendered, output_json)
}

fn render_schema_payload(request: ConfigRequest) -> Result<Option<String>, BuiltinError> {
    let color_enabled = builtin_output_color_enabled(request.output_json);
    let target = request.target;
    let bundle = request.bundle.clone();
    let runner = request.runner;

    let rendered = match target {
        Some(ConfigSchemaTarget::Bundle) => {
            let resolved_bundle = match bundle.as_deref() {
                Some(name) => Some(get_bundle(name).ok_or_else(|| {
                    BuiltinError::task_invocation(format!("unknown bundle `{name}`"))
                })?),
                None => None,
            };
            let default_paths = match bundle.as_deref() {
                Some(name) => list_bundle_default_paths(name)
                    .map_err(|error| BuiltinError::task_invocation(error.to_string()))?,
                None => Vec::new(),
            };

            render_builtin_config_schema_bundle_target(
                request.minimal,
                resolved_bundle.as_ref(),
                &default_paths,
            )
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
        ConfigPayload::schema(request.minimal, target, bundle, runner, text),
    )
}

fn render_reference_payload(output_json: bool) -> Result<Option<String>, BuiltinError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_config_payload(output_json, ConfigPayload::reference(rendered))
}

fn render_inspect_payload(
    request: ConfigRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    let manifest_path = target_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let selected_path = request.inspect_path.clone();
    let selected = selected_path
        .as_deref()
        .map(|path| inspect_selected_path(&loaded, path, &manifest_path))
        .transpose()?;
    let text = render_manifest_inspection(target_root, &loaded, selected.as_ref())?;
    let evaluation_order = loaded
        .evaluation_order
        .iter()
        .map(|path| display_path(path, target_root))
        .collect::<Vec<_>>();
    let include_graph = loaded
        .include_graph
        .iter()
        .map(|edge| {
            json!({
                "parent": display_path(&edge.parent, target_root),
                "child": display_path(&edge.child, target_root),
                "override_paths": edge.override_paths,
                "extend_paths": edge.extend_paths,
            })
        })
        .collect::<Vec<_>>();
    let overridden_paths = loaded
        .overridden_paths
        .iter()
        .map(|entry| {
            json!({
                "path": entry.path,
                "replaced_source": display_path(&entry.replaced_source, target_root),
                "by_fragment": display_path(&entry.by_fragment, target_root),
            })
        })
        .collect::<Vec<_>>();
    let value_sources = loaded
        .value_sources
        .iter()
        .map(|entry| {
            json!({
                "path": entry.path,
                "source": display_path(&entry.source, target_root),
            })
        })
        .collect::<Vec<_>>();

    render_config_payload(
        request.output_json,
        ConfigPayload::inspect(
            text,
            display_path(&loaded.manifest_path, target_root),
            evaluation_order,
            include_graph,
            overridden_paths,
            value_sources,
            loaded.effective_manifest,
            selected_path,
            selected.map(|entry| {
                json!({
                    "path": entry.path,
                    "source": display_path(&entry.source, target_root),
                    "value": toml_value_to_json(&entry.value),
                    "rendered": render_selected_value(&entry.path, &entry.value)
                        .unwrap_or_else(|_| "<render failed>".to_owned()),
                    "overrides": entry
                        .overrides
                        .iter()
                        .map(|override_entry| {
                            json!({
                                "path": override_entry.path,
                                "replaced_source": display_path(&override_entry.replaced_source, target_root),
                                "by_fragment": display_path(&override_entry.by_fragment, target_root),
                            })
                        })
                        .collect::<Vec<_>>(),
                })
            }),
        ),
    )
}

fn render_config_payload(
    output_json: bool,
    payload: ConfigPayload,
) -> Result<Option<String>, BuiltinError> {
    let mode = payload.mode;
    let minimal = payload.minimal;
    let target = payload.target.map(ConfigSchemaTarget::as_str);
    let bundle = payload.bundle;
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

struct ConfigPayload {
    mode: &'static str,
    minimal: bool,
    target: Option<ConfigSchemaTarget>,
    bundle: Option<String>,
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
    fn reference(text: String) -> Self {
        Self {
            mode: "reference",
            minimal: false,
            target: None,
            bundle: None,
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

    fn schema(
        minimal: bool,
        target: Option<ConfigSchemaTarget>,
        bundle: Option<String>,
        runner: Option<ConfigTestRunner>,
        text: String,
    ) -> Self {
        Self {
            mode: "schema",
            minimal,
            target,
            bundle,
            runner,
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

    fn inspect(
        text: String,
        manifest_path: String,
        evaluation_order: Vec<String>,
        include_graph: Vec<serde_json::Value>,
        overridden_paths: Vec<serde_json::Value>,
        value_sources: Vec<serde_json::Value>,
        effective_manifest: String,
        selected_path: Option<String>,
        selected_value: Option<serde_json::Value>,
    ) -> Self {
        Self {
            mode: "inspect",
            minimal: false,
            target: None,
            bundle: None,
            runner: None,
            manifest_path: Some(manifest_path),
            evaluation_order: Some(evaluation_order),
            include_graph: Some(include_graph),
            overridden_paths: Some(overridden_paths),
            value_sources: Some(value_sources),
            effective_manifest: Some(effective_manifest),
            selected_path,
            selected_value,
            user_config_path: None,
            user_config: None,
            user_config_removed: false,
            user_config_key: None,
            user_config_value: None,
            text,
        }
    }

    fn user_path(text: String, user_config_path: String) -> Self {
        Self {
            mode: "user_path",
            minimal: false,
            target: None,
            bundle: None,
            runner: None,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
            selected_path: None,
            selected_value: None,
            user_config_path: Some(user_config_path),
            user_config: None,
            user_config_removed: false,
            user_config_key: None,
            user_config_value: None,
            text,
        }
    }

    fn user_inspect(text: String, user_config_path: String, user_config: &UserConfig) -> Self {
        Self {
            mode: "user_inspect",
            minimal: false,
            target: None,
            bundle: None,
            runner: None,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
            selected_path: None,
            selected_value: None,
            user_config_path: Some(user_config_path),
            user_config: Some(serialize_user_config(user_config)),
            user_config_removed: false,
            user_config_key: None,
            user_config_value: None,
            text,
        }
    }

    fn user_get(
        text: String,
        user_config_path: String,
        user_config: &UserConfig,
        user_config_key: String,
        user_config_value: String,
    ) -> Self {
        Self {
            mode: "user_get",
            minimal: false,
            target: None,
            bundle: None,
            runner: None,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
            selected_path: None,
            selected_value: None,
            user_config_path: Some(user_config_path),
            user_config: Some(serialize_user_config(user_config)),
            user_config_removed: false,
            user_config_key: Some(user_config_key),
            user_config_value: Some(user_config_value),
            text,
        }
    }

    fn user_update(
        text: String,
        user_config_path: String,
        user_config: &UserConfig,
        user_config_removed: bool,
    ) -> Self {
        Self {
            mode: "user_update",
            minimal: false,
            target: None,
            bundle: None,
            runner: None,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
            selected_path: None,
            selected_value: None,
            user_config_path: Some(user_config_path),
            user_config: Some(serialize_user_config(user_config)),
            user_config_removed,
            user_config_key: None,
            user_config_value: None,
            text,
        }
    }
}

fn render_manifest_inspection(
    target_root: &Path,
    loaded: &effigy_manifest::LoadedTaskManifest,
    selected: Option<&SelectedInspectValue>,
) -> Result<String, BuiltinError> {
    let mut out = String::new();
    out.push_str("Manifest Composition\n");
    out.push_str("====================\n\n");
    out.push_str(&format!(
        "Root manifest: {}\n\n",
        display_path(&loaded.manifest_path, target_root)
    ));

    out.push_str("Evaluation Order\n");
    out.push_str("----------------\n");
    for (index, path) in loaded.evaluation_order.iter().enumerate() {
        out.push_str(&format!(
            "{}. {}\n",
            index + 1,
            display_path(path, target_root)
        ));
    }
    out.push('\n');

    out.push_str("Include Graph\n");
    out.push_str("-------------\n");
    if loaded.include_graph.is_empty() {
        out.push_str("(none)\n");
    } else {
        for edge in &loaded.include_graph {
            out.push_str(&render_edge(target_root, edge));
            out.push('\n');
        }
    }
    out.push('\n');

    out.push_str("Overridden Paths\n");
    out.push_str("----------------\n");
    if loaded.overridden_paths.is_empty() {
        out.push_str("(none)\n");
    } else {
        for entry in &loaded.overridden_paths {
            out.push_str(&render_override(target_root, entry));
            out.push('\n');
        }
    }
    out.push('\n');

    if let Some(selected) = selected {
        out.push_str("Selected Path\n");
        out.push_str("-------------\n");
        out.push_str(&format!("Path: {}\n", selected.path));
        out.push_str(&format!(
            "Source: {}\n",
            display_path(&selected.source, target_root)
        ));
        if selected.overrides.is_empty() {
            out.push_str("Overrides: (none)\n");
        } else {
            out.push_str("Overrides:\n");
            for entry in &selected.overrides {
                out.push_str(&format!(
                    "- {}: {} -> {}\n",
                    entry.path,
                    display_path(&entry.replaced_source, target_root),
                    display_path(&entry.by_fragment, target_root)
                ));
            }
        }
        out.push('\n');

        out.push_str("Selected Value\n");
        out.push_str("--------------\n");
        out.push_str(&render_selected_value(&selected.path, &selected.value)?);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    } else {
        out.push_str("Effective Value Sources\n");
        out.push_str("-----------------------\n");
        for line in render_grouped_value_sources(target_root, &loaded.value_sources) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push('\n');

    out.push_str("Effective Manifest\n");
    out.push_str("------------------\n");
    out.push_str(&loaded.effective_manifest);
    Ok(out)
}

fn render_edge(target_root: &Path, edge: &ManifestCompositionEdge) -> String {
    let head = format!(
        "- {} -> {}",
        display_path(&edge.parent, target_root),
        display_path(&edge.child, target_root)
    );
    let mut suffixes = Vec::new();
    if !edge.override_paths.is_empty() {
        suffixes.push(format!("override: {}", edge.override_paths.join(", ")));
    }
    if !edge.extend_paths.is_empty() {
        suffixes.push(format!("extend: {}", edge.extend_paths.join(", ")));
    }
    if suffixes.is_empty() {
        head
    } else {
        format!("{head} ({})", suffixes.join("; "))
    }
}

fn render_override(target_root: &Path, entry: &ManifestCompositionOverride) -> String {
    format!(
        "- {}: {} -> {}",
        entry.path,
        display_path(&entry.replaced_source, target_root),
        display_path(&entry.by_fragment, target_root)
    )
}

fn display_path(path: &Path, target_root: &Path) -> String {
    path.strip_prefix(target_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

#[derive(Debug, Clone)]
struct SelectedInspectValue {
    path: String,
    source: PathBuf,
    value: Value,
    overrides: Vec<ManifestCompositionOverride>,
}

fn inspect_selected_path(
    loaded: &effigy_manifest::LoadedTaskManifest,
    path: &str,
    manifest_path: &Path,
) -> Result<SelectedInspectValue, BuiltinError> {
    let value = lookup_value_at_path(&loaded.effective_value, path).ok_or_else(|| {
        BuiltinError::task_invocation(format!(
            "config path `{path}` was not found in the effective manifest"
        ))
    })?;
    let source = loaded
        .value_sources
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.source.clone())
        .ok_or_else(|| BuiltinError::TaskManifestCompose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "selected config path `{path}` exists in the effective manifest but has no source record"
            ),
        })?;
    let overrides = loaded
        .overridden_paths
        .iter()
        .filter(|entry| path == entry.path || path.starts_with(&format!("{}.", entry.path)))
        .cloned()
        .collect::<Vec<_>>();

    Ok(SelectedInspectValue {
        path: path.to_owned(),
        source,
        value: value.clone(),
        overrides,
    })
}

fn lookup_value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for segment in path.split('.') {
        let table = current.as_table()?;
        current = table.get(segment)?;
    }
    Some(current)
}

fn render_grouped_value_sources(
    target_root: &Path,
    entries: &[ManifestCompositionValueSource],
) -> Vec<String> {
    if entries.is_empty() {
        return vec!["(none)".to_owned()];
    }
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in entries {
        grouped
            .entry(display_path(&entry.source, target_root))
            .or_default()
            .push(entry.path.clone());
    }

    let mut out = Vec::new();
    for (source, mut paths) in grouped {
        paths.sort();
        out.push(format!("{source}:"));
        for path in paths {
            out.push(format!("- {path}"));
        }
    }
    out
}

fn render_selected_value(path: &str, value: &Value) -> Result<String, BuiltinError> {
    let mut wrapper = toml::map::Map::new();
    insert_value_at_path(&mut wrapper, path, value.clone());
    toml::to_string_pretty(&Value::Table(wrapper)).map_err(|error| {
        BuiltinError::task_invocation(format!(
            "failed to render selected config value `{path}`: {error}"
        ))
    })
}

fn insert_value_at_path(table: &mut toml::map::Map<String, Value>, path: &str, value: Value) {
    let mut segments = path.split('.').collect::<Vec<_>>();
    let last = segments
        .pop()
        .expect("selected config path should not be empty");
    let mut current = table;
    for segment in segments {
        let entry = current
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Table(toml::map::Map::new()));
        current = entry
            .as_table_mut()
            .expect("generated inspect wrapper should stay table-shaped");
    }
    current.insert(last.to_owned(), value);
}

fn toml_value_to_json(value: &Value) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

fn render_user_config_text(
    path: &Path,
    config: &UserConfig,
    removed_file: bool,
) -> Result<String, BuiltinError> {
    let mut out = String::new();
    out.push_str("User Config\n");
    out.push_str("===========\n\n");
    out.push_str(&format!("Path: {}\n", path.display()));
    if removed_file {
        out.push_str("Status: removed empty config file\n\n");
    } else if path.exists() {
        out.push_str("Status: present\n\n");
    } else {
        out.push_str("Status: not yet created\n\n");
    }

    out.push_str("Container Preferences\n");
    out.push_str("---------------------\n");
    out.push_str(&format!(
        "backend: {}\n",
        config
            .preferred_container_backend()
            .map(render_backend_preference)
            .unwrap_or("(unset)")
    ));
    out.push_str(&format!(
        "profile: {}\n",
        config.preferred_container_profile().unwrap_or("(unset)")
    ));

    if !config.bundle.is_empty() {
        out.push('\n');
        out.push_str("Rendered TOML\n");
        out.push_str("-------------\n");
        out.push_str(&render_user_config_toml(config)?);
    }

    Ok(out)
}

fn render_backend_preference(backend: UserContainerBackendPreference) -> &'static str {
    match backend {
        UserContainerBackendPreference::Containerd => "containerd",
        UserContainerBackendPreference::Docker => "docker",
    }
}

fn render_user_config_key_value(config: &UserConfig, key: UserConfigKey) -> Option<String> {
    match key {
        UserConfigKey::ContainersBackend => config
            .preferred_container_backend()
            .map(render_backend_preference)
            .map(str::to_owned),
        UserConfigKey::ContainersProfile => {
            config.preferred_container_profile().map(str::to_owned)
        }
    }
}

fn render_user_config_toml(config: &UserConfig) -> Result<String, BuiltinError> {
    toml::to_string_pretty(config).map_err(|error| {
        BuiltinError::task_invocation(format!("failed to render user-global config: {error}"))
    })
}

fn serialize_user_config(config: &UserConfig) -> serde_json::Value {
    serde_json::to_value(config).unwrap_or(serde_json::Value::Null)
}
