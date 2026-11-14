//! The `config inspect` view: the manifest composition graph, where each
//! effective value came from, and one selected path's value.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::json;
use toml::Value;

use crate::config::output::payload::{render_config_payload, ConfigPayload};
use crate::config::request::ConfigRequest;
use crate::BuiltinError;
use effigy_manifest::{
    load_task_manifest_with_inspection, BundleInputSource, ManifestCompositionEdge,
    ManifestCompositionOverride, ManifestCompositionValueSource,
};

pub(super) fn render_inspect_payload(
    request: ConfigRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    let manifest_path = target_root.join(effigy_manifest::TASK_MANIFEST_FILE);
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
        ConfigPayload::inspect(ConfigInspect {
            text,
            manifest_path: display_path(&loaded.manifest_path, target_root),
            evaluation_order,
            include_graph,
            overridden_paths,
            value_sources,
            effective_manifest: loaded.effective_manifest,
            bundle_inputs: loaded.bundle_name.as_deref().map(|name| {
                json!({
                    "bundle": name,
                    "inputs": loaded
                        .bundle_inputs
                        .iter()
                        .map(|input| json!({
                            "name": input.name,
                            "value": toml_value_to_json(&input.value),
                            "source": input.source,
                        }))
                        .collect::<Vec<_>>(),
                })
            }),
            selected_path,
            // A value the text form cannot render fails the inspection;
            // `--json` never reports a placeholder instead.
            selected_value: selected
                .as_ref()
                .map(|entry| {
                    Ok::<_, BuiltinError>(json!({
                        "path": entry.path,
                        "source": display_path(&entry.source, target_root),
                        "value": toml_value_to_json(&entry.value),
                        "rendered": render_selected_value(&entry.path, &entry.value)?,
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
                    }))
                })
                .transpose()?,
        }),
    )
}

/// What `config inspect` reported: the text view plus the typed facts the JSON
/// envelope carries.
pub(super) struct ConfigInspect {
    pub(super) text: String,
    pub(super) manifest_path: String,
    pub(super) evaluation_order: Vec<String>,
    pub(super) include_graph: Vec<serde_json::Value>,
    pub(super) overridden_paths: Vec<serde_json::Value>,
    pub(super) value_sources: Vec<serde_json::Value>,
    pub(super) effective_manifest: String,
    pub(super) bundle_inputs: Option<serde_json::Value>,
    pub(super) selected_path: Option<String>,
    pub(super) selected_value: Option<serde_json::Value>,
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

    if let Some(name) = loaded.bundle_name.as_deref() {
        out.push_str("Bundle Inputs\n");
        out.push_str("-------------\n");
        out.push_str(&format!("bundle: {name}\n"));
        if loaded.bundle_inputs.is_empty() {
            out.push_str("(bundle declares no inputs)\n");
        } else {
            for input in &loaded.bundle_inputs {
                out.push_str(&format!(
                    "- {} = {} ({})\n",
                    input.name,
                    render_bundle_input_value(&input.value),
                    render_bundle_input_source(input.source),
                ));
            }
        }
        out.push('\n');
    }

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

/// One resolved bundle input value as `config inspect` text reads it: strings
/// bare, everything else in TOML form.
fn render_bundle_input_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Where a resolved bundle input value came from, in the words the text view
/// uses.
fn render_bundle_input_source(source: BundleInputSource) -> &'static str {
    match source {
        BundleInputSource::Default => "bundle default",
        BundleInputSource::UserConfig => "user config",
        BundleInputSource::Manifest => "manifest",
    }
}
