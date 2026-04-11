use serde_json::json;
use std::path::Path;

use super::super::render_builtin_help_text;
use super::super::response::{
    builtin_output_color_enabled, render_optional_text_with_schema_text_fields_lazy,
};
use super::reference::{render_config_reference, style_schema_comments};
use super::request::{ConfigRequest, ConfigSchemaTarget, ConfigTestRunner};
use super::schema::{
    render_builtin_config_schema, render_builtin_config_schema_minimal,
    render_builtin_config_schema_target, render_builtin_config_schema_test_target,
};
use crate::runner::error::RunnerError;
use crate::runner::manifest::{
    load_task_manifest_with_inspection, ManifestCompositionEdge, ManifestCompositionOverride,
    ManifestCompositionValueSource,
};

pub(super) fn render_config_request(
    request: ConfigRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    if request.inspect {
        return render_inspect_payload(request.output_json, target_root);
    }
    if request.schema {
        return render_schema_payload(request);
    }
    render_reference_payload(request.output_json)
}

pub(super) fn render_config_help_payload(output_json: bool) -> Result<String, RunnerError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_builtin_help_text("config", rendered, output_json)
}

fn render_schema_payload(request: ConfigRequest) -> Result<Option<String>, RunnerError> {
    let color_enabled = builtin_output_color_enabled(request.output_json);
    let target = request.target;
    let runner = request.runner;

    let rendered = match target {
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
        ConfigPayload::schema(request.minimal, target, runner, text),
    )
}

fn render_reference_payload(output_json: bool) -> Result<Option<String>, RunnerError> {
    let color_enabled = builtin_output_color_enabled(output_json);
    let rendered = render_config_reference(color_enabled)?;
    render_config_payload(output_json, ConfigPayload::reference(rendered))
}

fn render_inspect_payload(
    output_json: bool,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    let manifest_path = target_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let text = render_manifest_inspection(target_root, &loaded)?;
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
            })
        })
        .collect::<Vec<_>>();
    let overridden_paths = loaded
        .overridden_paths
        .iter()
        .map(|entry| {
            json!({
                "path": entry.path,
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
        output_json,
        ConfigPayload::inspect(
            text,
            display_path(&loaded.manifest_path, target_root),
            evaluation_order,
            include_graph,
            overridden_paths,
            value_sources,
            loaded.effective_manifest,
        ),
    )
}

fn render_config_payload(
    output_json: bool,
    payload: ConfigPayload,
) -> Result<Option<String>, RunnerError> {
    let mode = payload.mode;
    let minimal = payload.minimal;
    let target = payload.target.map(ConfigSchemaTarget::as_str);
    let runner = payload.runner.map(ConfigTestRunner::as_str);
    let manifest_path = payload.manifest_path;
    let evaluation_order = payload.evaluation_order;
    let include_graph = payload.include_graph;
    let overridden_paths = payload.overridden_paths;
    let value_sources = payload.value_sources;
    let effective_manifest = payload.effective_manifest;
    render_optional_text_with_schema_text_fields_lazy(
        output_json,
        "effigy.config.v1",
        move || payload.text,
        move || {
            json!({
                "mode": mode,
                "minimal": minimal,
                "target": target,
                "runner": runner,
                "manifest_path": manifest_path,
                "evaluation_order": evaluation_order,
                "include_graph": include_graph,
                "overridden_paths": overridden_paths,
                "value_sources": value_sources,
                "effective_manifest": effective_manifest,
            })
        },
    )
}

struct ConfigPayload {
    mode: &'static str,
    minimal: bool,
    target: Option<ConfigSchemaTarget>,
    runner: Option<ConfigTestRunner>,
    manifest_path: Option<String>,
    evaluation_order: Option<Vec<String>>,
    include_graph: Option<Vec<serde_json::Value>>,
    overridden_paths: Option<Vec<serde_json::Value>>,
    value_sources: Option<Vec<serde_json::Value>>,
    effective_manifest: Option<String>,
    text: String,
}

impl ConfigPayload {
    fn reference(text: String) -> Self {
        Self {
            mode: "reference",
            minimal: false,
            target: None,
            runner: None,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
            text,
        }
    }

    fn schema(
        minimal: bool,
        target: Option<ConfigSchemaTarget>,
        runner: Option<ConfigTestRunner>,
        text: String,
    ) -> Self {
        Self {
            mode: "schema",
            minimal,
            target,
            runner,
            manifest_path: None,
            evaluation_order: None,
            include_graph: None,
            overridden_paths: None,
            value_sources: None,
            effective_manifest: None,
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
    ) -> Self {
        Self {
            mode: "inspect",
            minimal: false,
            target: None,
            runner: None,
            manifest_path: Some(manifest_path),
            evaluation_order: Some(evaluation_order),
            include_graph: Some(include_graph),
            overridden_paths: Some(overridden_paths),
            value_sources: Some(value_sources),
            effective_manifest: Some(effective_manifest),
            text,
        }
    }
}

fn render_manifest_inspection(
    target_root: &Path,
    loaded: &crate::runner::manifest::LoadedTaskManifest,
) -> Result<String, RunnerError> {
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

    out.push_str("Effective Value Sources\n");
    out.push_str("-----------------------\n");
    for entry in &loaded.value_sources {
        out.push_str(&render_value_source(target_root, entry));
        out.push('\n');
    }
    out.push('\n');

    out.push_str("Effective Manifest\n");
    out.push_str("------------------\n");
    out.push_str(&loaded.effective_manifest);
    Ok(out)
}

fn render_edge(target_root: &Path, edge: &ManifestCompositionEdge) -> String {
    if edge.override_paths.is_empty() {
        return format!(
            "- {} -> {}",
            display_path(&edge.parent, target_root),
            display_path(&edge.child, target_root)
        );
    }
    format!(
        "- {} -> {} (override: {})",
        display_path(&edge.parent, target_root),
        display_path(&edge.child, target_root),
        edge.override_paths.join(", ")
    )
}

fn render_override(target_root: &Path, entry: &ManifestCompositionOverride) -> String {
    format!(
        "- {} <- {}",
        entry.path,
        display_path(&entry.by_fragment, target_root)
    )
}

fn render_value_source(target_root: &Path, entry: &ManifestCompositionValueSource) -> String {
    format!(
        "- {} <- {}",
        entry.path,
        display_path(&entry.source, target_root)
    )
}

fn display_path(path: &Path, target_root: &Path) -> String {
    path.strip_prefix(target_root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
