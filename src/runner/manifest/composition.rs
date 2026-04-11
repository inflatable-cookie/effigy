use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Value;

use super::TaskManifest;
use crate::runner::error::RunnerError;

#[derive(Debug)]
pub(in crate::runner) struct LoadedTaskManifest {
    pub(in crate::runner) manifest: TaskManifest,
    pub(in crate::runner) effective_value: Value,
    pub(in crate::runner) manifest_path: PathBuf,
    pub(in crate::runner) evaluation_order: Vec<PathBuf>,
    pub(in crate::runner) include_graph: Vec<ManifestCompositionEdge>,
    pub(in crate::runner) overridden_paths: Vec<ManifestCompositionOverride>,
    pub(in crate::runner) value_sources: Vec<ManifestCompositionValueSource>,
    pub(in crate::runner) effective_manifest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(in crate::runner) struct ManifestCompositionEdge {
    pub(in crate::runner) parent: PathBuf,
    pub(in crate::runner) child: PathBuf,
    pub(in crate::runner) override_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(in crate::runner) struct ManifestCompositionOverride {
    pub(in crate::runner) path: String,
    pub(in crate::runner) replaced_source: PathBuf,
    pub(in crate::runner) by_fragment: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub(in crate::runner) struct ManifestCompositionValueSource {
    pub(in crate::runner) path: String,
    pub(in crate::runner) source: PathBuf,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSectionConfig {
    #[serde(default)]
    include: Vec<ManifestIncludeEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ManifestIncludeEntry {
    Path(String),
    Detailed(ManifestIncludeDirective),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestIncludeDirective {
    path: String,
    #[serde(default, rename = "override")]
    override_paths: Vec<String>,
}

#[derive(Debug, Clone)]
struct ManifestIncludeSpec {
    resolved_path: PathBuf,
    override_paths: Vec<String>,
}

#[derive(Debug, Clone)]
struct ComposedValue {
    value: Value,
    source_map: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Default)]
struct CompositionSession {
    stack: Vec<PathBuf>,
    evaluation_order: Vec<PathBuf>,
    include_graph: Vec<ManifestCompositionEdge>,
    overridden_paths: Vec<ManifestCompositionOverride>,
}

pub(in crate::runner) fn load_task_manifest_with_inspection(
    manifest_path: &Path,
) -> Result<LoadedTaskManifest, RunnerError> {
    let mut session = CompositionSession::default();
    let composed = load_composed_value(manifest_path, &mut session)?;
    let manifest =
        composed
            .value
            .clone()
            .try_into()
            .map_err(|error| RunnerError::TaskManifestParse {
                path: manifest_path.to_path_buf(),
                error,
            })?;
    let effective_manifest = toml::to_string_pretty(&composed.value)
        .map_err(|error| RunnerError::task_invocation_failed_render(manifest_path, error))?;
    let value_sources = composed
        .source_map
        .into_iter()
        .map(|(path, source)| ManifestCompositionValueSource { path, source })
        .collect::<Vec<_>>();

    Ok(LoadedTaskManifest {
        manifest,
        effective_value: composed.value,
        manifest_path: manifest_path.to_path_buf(),
        evaluation_order: session.evaluation_order,
        include_graph: session.include_graph,
        overridden_paths: session.overridden_paths,
        value_sources,
        effective_manifest,
    })
}

fn load_composed_value(
    manifest_path: &Path,
    session: &mut CompositionSession,
) -> Result<ComposedValue, RunnerError> {
    let identity = canonical_manifest_identity(manifest_path);
    if let Some(cycle_start) = session.stack.iter().position(|path| path == &identity) {
        let cycle = session.stack[cycle_start..]
            .iter()
            .chain(std::iter::once(&identity))
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(RunnerError::TaskManifestCompose {
            path: manifest_path.to_path_buf(),
            detail: format!("manifest include cycle detected: {cycle}"),
        });
    }

    session.stack.push(identity);
    session.evaluation_order.push(manifest_path.to_path_buf());

    let source =
        std::fs::read_to_string(manifest_path).map_err(|error| RunnerError::TaskManifestRead {
            path: manifest_path.to_path_buf(),
            error,
        })?;
    let raw = toml::from_str::<Value>(&source).map_err(|error| RunnerError::TaskManifestParse {
        path: manifest_path.to_path_buf(),
        error,
    })?;
    let mut table = raw
        .as_table()
        .cloned()
        .ok_or_else(|| RunnerError::TaskManifestCompose {
            path: manifest_path.to_path_buf(),
            detail: "manifest root must be a TOML table".to_owned(),
        })?;

    let includes = take_include_specs(manifest_path, &mut table)?;
    let mut composed = ComposedValue {
        value: Value::Table(table),
        source_map: BTreeMap::new(),
    };
    record_value_sources("", &composed.value, manifest_path, &mut composed.source_map);

    for include in includes {
        let child = load_composed_value(&include.resolved_path, session)?;
        session.include_graph.push(ManifestCompositionEdge {
            parent: manifest_path.to_path_buf(),
            child: include.resolved_path.clone(),
            override_paths: include.override_paths.clone(),
        });
        merge_values(
            "",
            &mut composed.value,
            &child.value,
            &child.source_map,
            &include,
            &mut session.overridden_paths,
            &mut composed.source_map,
            manifest_path,
        )?;
    }

    session.stack.pop();
    Ok(composed)
}

fn take_include_specs(
    manifest_path: &Path,
    table: &mut toml::map::Map<String, Value>,
) -> Result<Vec<ManifestIncludeSpec>, RunnerError> {
    let Some(section) = table.remove("manifest") else {
        return Ok(Vec::new());
    };
    let config: ManifestSectionConfig =
        section
            .try_into()
            .map_err(|error| RunnerError::TaskManifestCompose {
                path: manifest_path.to_path_buf(),
                detail: format!("invalid `[manifest]` section: {error}"),
            })?;
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut specs = Vec::with_capacity(config.include.len());
    for entry in config.include {
        let (path, override_paths) = match entry {
            ManifestIncludeEntry::Path(path) => (path, Vec::new()),
            ManifestIncludeEntry::Detailed(detail) => (detail.path, detail.override_paths),
        };
        let resolved_path = if Path::new(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            parent.join(&path)
        };
        specs.push(ManifestIncludeSpec {
            resolved_path,
            override_paths,
        });
    }
    Ok(specs)
}

fn merge_values(
    path: &str,
    current: &mut Value,
    incoming: &Value,
    incoming_sources: &BTreeMap<String, PathBuf>,
    include: &ManifestIncludeSpec,
    overridden_paths: &mut Vec<ManifestCompositionOverride>,
    current_sources: &mut BTreeMap<String, PathBuf>,
    root_manifest_path: &Path,
) -> Result<(), RunnerError> {
    let override_set = include
        .override_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut used_overrides = BTreeSet::new();
    merge_value_inner(
        path,
        current,
        incoming,
        incoming_sources,
        &override_set,
        &mut used_overrides,
        overridden_paths,
        current_sources,
        &include.resolved_path,
        root_manifest_path,
    )?;
    let unused = include
        .override_paths
        .iter()
        .filter(|path| !used_overrides.contains((*path).as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unused.is_empty() {
        return Err(RunnerError::TaskManifestCompose {
            path: root_manifest_path.to_path_buf(),
            detail: format!(
                "unused override path(s) for {}: {}",
                include.resolved_path.display(),
                unused.join(", ")
            ),
        });
    }
    Ok(())
}

fn merge_value_inner(
    path: &str,
    current: &mut Value,
    incoming: &Value,
    incoming_sources: &BTreeMap<String, PathBuf>,
    override_set: &BTreeSet<String>,
    used_overrides: &mut BTreeSet<String>,
    overridden_paths: &mut Vec<ManifestCompositionOverride>,
    current_sources: &mut BTreeMap<String, PathBuf>,
    incoming_fragment: &Path,
    root_manifest_path: &Path,
) -> Result<(), RunnerError> {
    if !path.is_empty() && override_set.contains(path) {
        let existing_source =
            current_value_source(path, current_sources, root_manifest_path).to_path_buf();
        if value_kind(current) != value_kind(incoming) {
            return Err(RunnerError::TaskManifestCompose {
                path: root_manifest_path.to_path_buf(),
                detail: format!(
                    "override path `{path}` cannot replace {} from {} with {} from {}",
                    value_kind(current),
                    existing_source.display(),
                    value_kind(incoming),
                    incoming_fragment.display()
                ),
            });
        }
        *current = incoming.clone();
        remove_source_entries(path, current_sources);
        copy_source_entries(
            path,
            incoming_sources,
            current_sources,
            incoming_fragment,
            incoming,
        );
        overridden_paths.push(ManifestCompositionOverride {
            path: path.to_owned(),
            replaced_source: existing_source,
            by_fragment: incoming_fragment.to_path_buf(),
        });
        used_overrides.insert(path.to_owned());
        return Ok(());
    }

    match (current.as_table_mut(), incoming.as_table()) {
        (Some(current_table), Some(incoming_table)) => {
            for (key, incoming_value) in incoming_table {
                let child_path = join_path(path, key);
                match current_table.get_mut(key) {
                    Some(current_value) => merge_value_inner(
                        &child_path,
                        current_value,
                        incoming_value,
                        incoming_sources,
                        override_set,
                        used_overrides,
                        overridden_paths,
                        current_sources,
                        incoming_fragment,
                        root_manifest_path,
                    )?,
                    None => {
                        current_table.insert(key.clone(), incoming_value.clone());
                        copy_source_entries(
                            &child_path,
                            incoming_sources,
                            current_sources,
                            incoming_fragment,
                            incoming_value,
                        );
                    }
                }
            }
            return Ok(());
        }
        (Some(_), None) | (None, Some(_)) => {
            let existing_source =
                current_value_source(path, current_sources, root_manifest_path).to_path_buf();
            return Err(RunnerError::TaskManifestCompose {
                path: root_manifest_path.to_path_buf(),
                detail: format!(
                    "manifest conflict at `{}` between {} from {} and {} from {}",
                    display_path(path),
                    value_kind(current),
                    existing_source.display(),
                    value_kind(incoming),
                    incoming_fragment.display()
                ),
            });
        }
        (None, None) => {}
    }

    if current == incoming {
        return Ok(());
    }

    let existing_source =
        current_value_source(path, current_sources, root_manifest_path).to_path_buf();
    Err(RunnerError::TaskManifestCompose {
        path: root_manifest_path.to_path_buf(),
        detail: format!(
            "manifest conflict at `{}` between {} and {}; add override = [\"{}\"] to the include entry for {}",
            display_path(path),
            existing_source.display(),
            incoming_fragment.display(),
            path,
            incoming_fragment.display(),
        ),
    })
}

fn canonical_manifest_identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn join_path(base: &str, key: &str) -> String {
    if base.is_empty() {
        key.to_owned()
    } else {
        format!("{base}.{key}")
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() {
        "<root>"
    } else {
        path
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "array",
        Value::Table(_) => "table",
    }
}

fn record_value_sources(
    path: &str,
    value: &Value,
    source: &Path,
    out: &mut BTreeMap<String, PathBuf>,
) {
    if !path.is_empty() {
        out.insert(path.to_owned(), source.to_path_buf());
    }
    if let Some(table) = value.as_table() {
        for (key, child) in table {
            record_value_sources(&join_path(path, key), child, source, out);
        }
    }
}

fn remove_source_entries(path: &str, source_map: &mut BTreeMap<String, PathBuf>) {
    let prefix = format!("{path}.");
    source_map.retain(|key, _| key != path && !key.starts_with(&prefix));
}

fn copy_source_entries(
    path: &str,
    incoming_sources: &BTreeMap<String, PathBuf>,
    current_sources: &mut BTreeMap<String, PathBuf>,
    fallback_source: &Path,
    incoming_value: &Value,
) {
    let prefix = format!("{path}.");
    let mut copied_any = false;
    for (source_path, source_file) in incoming_sources {
        if source_path == path || source_path.starts_with(&prefix) {
            current_sources.insert(source_path.clone(), source_file.clone());
            copied_any = true;
        }
    }
    if !copied_any {
        record_value_sources(path, incoming_value, fallback_source, current_sources);
    }
}

fn current_value_source<'a>(
    path: &str,
    current_sources: &'a BTreeMap<String, PathBuf>,
    root_manifest_path: &'a Path,
) -> &'a Path {
    if path.is_empty() {
        return root_manifest_path;
    }
    current_sources
        .get(path)
        .map(PathBuf::as_path)
        .unwrap_or(root_manifest_path)
}
