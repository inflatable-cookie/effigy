use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Value;

use super::TaskManifest;
use crate::bundles::apply_bundle_defaults;
use crate::ManifestError;

#[derive(Debug)]
pub struct LoadedTaskManifest {
    pub manifest: TaskManifest,
    pub effective_value: Value,
    pub manifest_path: PathBuf,
    pub evaluation_order: Vec<PathBuf>,
    pub include_graph: Vec<ManifestCompositionEdge>,
    pub overridden_paths: Vec<ManifestCompositionOverride>,
    pub value_sources: Vec<ManifestCompositionValueSource>,
    pub effective_manifest: String,
    pub bundle_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestCompositionEdge {
    pub parent: PathBuf,
    pub child: PathBuf,
    pub override_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extend_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManifestCompositionOverride {
    pub path: String,
    pub replaced_source: PathBuf,
    pub by_fragment: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct ManifestCompositionValueSource {
    pub path: String,
    pub source: PathBuf,
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
    #[serde(default, rename = "extend")]
    extend_paths: Vec<String>,
    #[serde(default)]
    optional: bool,
}

#[derive(Debug, Clone)]
struct ManifestIncludeSpec {
    resolved_path: PathBuf,
    override_paths: Vec<String>,
    extend_paths: Vec<String>,
    optional: bool,
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

pub fn load_task_manifest_with_inspection(
    manifest_path: &Path,
) -> Result<LoadedTaskManifest, ManifestError> {
    let mut session = CompositionSession::default();
    let mut composed = load_composed_value(manifest_path, &mut session)?;
    let bundle_defaults = apply_bundle_defaults(manifest_path, &mut composed.value)?;
    if let Some(bundle_defaults) = bundle_defaults.as_ref() {
        record_missing_bundle_sources(
            "",
            &composed.value,
            &bundle_defaults.source_path,
            &mut composed.source_map,
        );
    }
    let manifest: TaskManifest =
        composed
            .value
            .clone()
            .try_into()
            .map_err(|error| ManifestError::Parse {
                path: manifest_path.to_path_buf(),
                error,
            })?;
    manifest.validate(manifest_path)?;
    let effective_manifest =
        toml::to_string_pretty(&composed.value).map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: error.to_string(),
        })?;
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
        bundle_root: bundle_defaults.map(|defaults| defaults.bundle_root),
    })
}

fn load_composed_value(
    manifest_path: &Path,
    session: &mut CompositionSession,
) -> Result<ComposedValue, ManifestError> {
    let identity = canonical_manifest_identity(manifest_path);
    if let Some(cycle_start) = session.stack.iter().position(|path| path == &identity) {
        let cycle = session.stack[cycle_start..]
            .iter()
            .chain(std::iter::once(&identity))
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("manifest include cycle detected: {cycle}"),
        });
    }

    session.stack.push(identity);
    session.evaluation_order.push(manifest_path.to_path_buf());

    let source = std::fs::read_to_string(manifest_path).map_err(|error| ManifestError::Read {
        path: manifest_path.to_path_buf(),
        error,
    })?;
    let raw = toml::from_str::<Value>(&source).map_err(|error| ManifestError::Parse {
        path: manifest_path.to_path_buf(),
        error,
    })?;
    let mut table = raw
        .as_table()
        .cloned()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "manifest root must be a TOML table".to_owned(),
        })?;

    let mut includes = take_include_specs(manifest_path, &mut table)?;
    if session.stack.len() == 1 {
        append_local_overlay_include(manifest_path, &mut includes);
    }
    let mut composed = ComposedValue {
        value: Value::Table(table),
        source_map: BTreeMap::new(),
    };
    record_value_sources("", &composed.value, manifest_path, &mut composed.source_map);

    for include in includes {
        if include.optional && !include.resolved_path.exists() {
            continue;
        }
        let child = load_composed_value(&include.resolved_path, session)?;
        session.include_graph.push(ManifestCompositionEdge {
            parent: manifest_path.to_path_buf(),
            child: include.resolved_path.clone(),
            override_paths: include.override_paths.clone(),
            extend_paths: include.extend_paths.clone(),
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

/// Filename of the auto-discovered local-overlay manifest.
const LOCAL_OVERLAY_FILENAME: &str = "effigy.local.toml";
/// Env switch that disables auto-discovery for CI determinism.
const NO_LOCAL_OVERLAY_ENV: &str = "EFFIGY_NO_LOCAL_OVERLAY";

/// Appends a synthetic optional include for `effigy.local.toml` when one
/// is present alongside the root manifest, unless it's already declared
/// explicitly or `EFFIGY_NO_LOCAL_OVERLAY=1` is set. Idempotent: a no-op
/// when the file is missing.
fn append_local_overlay_include(manifest_path: &Path, includes: &mut Vec<ManifestIncludeSpec>) {
    if std::env::var(NO_LOCAL_OVERLAY_ENV).ok().as_deref() == Some("1") {
        return;
    }
    let parent = match manifest_path.parent() {
        Some(parent) => parent,
        None => return,
    };
    let local_path = parent.join(LOCAL_OVERLAY_FILENAME);
    if !local_path.is_file() {
        return;
    }
    let canonical_local = std::fs::canonicalize(&local_path).unwrap_or_else(|_| local_path.clone());
    let already_declared = includes.iter().any(|spec| {
        let canonical_spec = std::fs::canonicalize(&spec.resolved_path)
            .unwrap_or_else(|_| spec.resolved_path.clone());
        canonical_spec == canonical_local
    });
    if already_declared {
        return;
    }
    // Best-effort: amend `.gitignore` so the local overlay is never
    // committed accidentally. Failures here are non-fatal — manifest
    // loading should not depend on filesystem write access.
    if let Some(repo_root) = locate_git_root(parent) {
        let _ = effigy_core::runtime_dir::ensure_local_overlay_ignored_in_git_root(&repo_root);
    }
    includes.push(ManifestIncludeSpec {
        resolved_path: local_path,
        override_paths: Vec::new(),
        extend_paths: Vec::new(),
        optional: true,
    });
}

/// Walk upward from `start` looking for a `.git` dir. Returns the
/// directory containing it, if any.
fn locate_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").is_dir() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn take_include_specs(
    manifest_path: &Path,
    table: &mut toml::map::Map<String, Value>,
) -> Result<Vec<ManifestIncludeSpec>, ManifestError> {
    let Some(section) = table.remove("manifest") else {
        return Ok(Vec::new());
    };
    let config: ManifestSectionConfig =
        section.try_into().map_err(|error| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid `[manifest]` section: {error}"),
        })?;
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut specs = Vec::with_capacity(config.include.len());
    for entry in config.include {
        let (path, override_paths, extend_paths, optional) = match entry {
            ManifestIncludeEntry::Path(path) => (path, Vec::new(), Vec::new(), false),
            ManifestIncludeEntry::Detailed(detail) => (
                detail.path,
                detail.override_paths,
                detail.extend_paths,
                detail.optional,
            ),
        };
        let resolved_path = if Path::new(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            parent.join(&path)
        };
        if let Some(conflict) = override_paths.iter().find(|p| extend_paths.contains(p)) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "include `{}` declares `{}` in both `override` and `extend`; pick one",
                    resolved_path.display(),
                    conflict
                ),
            });
        }
        specs.push(ManifestIncludeSpec {
            resolved_path,
            override_paths,
            extend_paths,
            optional,
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
) -> Result<(), ManifestError> {
    let override_set = include
        .override_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let extend_set = include
        .extend_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut used_overrides = BTreeSet::new();
    let mut used_extends = BTreeSet::new();
    merge_value_inner(
        path,
        current,
        incoming,
        incoming_sources,
        &override_set,
        &extend_set,
        &mut used_overrides,
        &mut used_extends,
        overridden_paths,
        current_sources,
        &include.resolved_path,
        root_manifest_path,
    )?;
    let unused_overrides = include
        .override_paths
        .iter()
        .filter(|path| !used_overrides.contains((*path).as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unused_overrides.is_empty() {
        return Err(ManifestError::Compose {
            path: root_manifest_path.to_path_buf(),
            detail: format!(
                "unused override path(s) for {}: {}",
                include.resolved_path.display(),
                unused_overrides.join(", ")
            ),
        });
    }
    // extend is prophylactic: declaring a path that didn't actually
    // need to be appended (because it didn't exist on the parent yet)
    // is the happy outcome, not an error. Override is strict because a
    // replacement that replaced nothing is almost always a typo;
    // extend doesn't carry the same risk.
    let _ = used_extends;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn merge_value_inner(
    path: &str,
    current: &mut Value,
    incoming: &Value,
    incoming_sources: &BTreeMap<String, PathBuf>,
    override_set: &BTreeSet<String>,
    extend_set: &BTreeSet<String>,
    used_overrides: &mut BTreeSet<String>,
    used_extends: &mut BTreeSet<String>,
    overridden_paths: &mut Vec<ManifestCompositionOverride>,
    current_sources: &mut BTreeMap<String, PathBuf>,
    incoming_fragment: &Path,
    root_manifest_path: &Path,
) -> Result<(), ManifestError> {
    if !path.is_empty() && extend_set.contains(path) {
        let existing_source =
            current_value_source(path, current_sources, root_manifest_path).to_path_buf();
        match (current.as_array_mut(), incoming.as_array()) {
            (Some(current_array), Some(incoming_array)) => {
                current_array.extend(incoming_array.iter().cloned());
                used_extends.insert(path.to_owned());
                return Ok(());
            }
            _ => {
                return Err(ManifestError::Compose {
                    path: root_manifest_path.to_path_buf(),
                    detail: format!(
                        "extend path `{path}` requires arrays on both sides; got {} from {} and {} from {}",
                        value_kind(current),
                        existing_source.display(),
                        value_kind(incoming),
                        incoming_fragment.display()
                    ),
                });
            }
        }
    }

    if !path.is_empty() && override_set.contains(path) {
        let existing_source =
            current_value_source(path, current_sources, root_manifest_path).to_path_buf();
        if value_kind(current) != value_kind(incoming) {
            return Err(ManifestError::Compose {
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
                        extend_set,
                        used_overrides,
                        used_extends,
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
            return Err(ManifestError::Compose {
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
    Err(ManifestError::Compose {
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

fn record_missing_bundle_sources(
    path: &str,
    value: &Value,
    source: &Path,
    out: &mut BTreeMap<String, PathBuf>,
) {
    if !path.is_empty() {
        out.entry(path.to_owned())
            .or_insert_with(|| source.to_path_buf());
    }
    if let Some(table) = value.as_table() {
        for (key, child) in table {
            record_missing_bundle_sources(&join_path(path, key), child, source, out);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_manifest(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write manifest");
        path
    }

    fn array_strings(value: &Value, path: &str) -> Vec<String> {
        let mut current = value;
        for segment in path.split('.') {
            current = current
                .as_table()
                .and_then(|table| table.get(segment))
                .unwrap_or_else(|| panic!("missing path segment {segment} in {path}"));
        }
        current
            .as_array()
            .expect("array")
            .iter()
            .map(|item| item.as_str().expect("string").to_owned())
            .collect()
    }

    #[test]
    fn extend_appends_array_entries() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml", extend = ["isolation.paths"] },
]

[isolation]
paths = ["a", "b"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[isolation]
paths = ["c", "d"]
"#,
        );

        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "isolation.paths");
        assert_eq!(
            domains,
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
            ]
        );
        let edge = loaded
            .include_graph
            .iter()
            .find(|edge| edge.child.file_name() == Some(std::ffi::OsStr::new("overlay.toml")))
            .expect("overlay edge");
        assert_eq!(edge.extend_paths, vec!["isolation.paths"]);
    }

    #[test]
    fn extend_on_non_array_path_errors() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml", extend = ["shell.run"] },
]

[shell]
run = "primary"
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[shell]
run = "secondary"
"#,
        );

        let err = load_task_manifest_with_inspection(&root).unwrap_err();
        let detail = format!("{err}");
        assert!(detail.contains("extend path `shell.run`"), "{detail}");
        assert!(detail.contains("requires arrays on both sides"), "{detail}");
        assert!(detail.contains("effigy.toml"), "{detail}");
        assert!(detail.contains("overlay.toml"), "{detail}");
    }

    #[test]
    fn extend_and_override_on_same_path_errors() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml",
    extend = ["isolation.paths"],
    override = ["isolation.paths"] },
]

[isolation]
paths = ["a"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[isolation]
paths = ["b"]
"#,
        );

        let err = load_task_manifest_with_inspection(&root).unwrap_err();
        let detail = format!("{err}");
        assert!(
            detail.contains("declares `isolation.paths` in both `override` and `extend`"),
            "{detail}"
        );
    }

    #[test]
    fn extend_on_non_conflicting_path_is_silent_noop() {
        // extend is prophylactic — listing a path that didn't actually
        // need appending (because the parent didn't have it yet) is
        // the happy outcome, not an error.
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml", extend = ["isolation.paths"] },
]
"#,
        );
        write_manifest(dir, "overlay.toml", "[isolation]\npaths = [\"a\"]\n");

        load_task_manifest_with_inspection(&root)
            .expect("non-conflicting extend should compose cleanly");
    }

    #[test]
    fn optional_missing_include_is_skipped() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "missing.toml", optional = true },
]

[isolation]
paths = ["a"]
"#,
        );
        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "isolation.paths");
        assert_eq!(domains, vec!["a".to_owned()]);
        // Optional skipped includes do not show up in the include graph.
        assert!(
            loaded.include_graph.is_empty(),
            "include_graph: {:?}",
            loaded.include_graph
        );
    }

    #[test]
    fn optional_missing_include_without_flag_errors() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "missing.toml" },
]
"#,
        );
        let err = load_task_manifest_with_inspection(&root).unwrap_err();
        let detail = format!("{err}");
        assert!(detail.contains("missing.toml"), "{detail}");
    }

    #[test]
    fn optional_present_include_is_loaded() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml", optional = true, extend = ["isolation.paths"] },
]

[isolation]
paths = ["a"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[isolation]
paths = ["b"]
"#,
        );
        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "isolation.paths");
        assert_eq!(domains, vec!["a".to_owned(), "b".to_owned()]);
    }

    /// Serialise tests that toggle process-global `EFFIGY_NO_LOCAL_OVERLAY`.
    static LOCAL_OVERLAY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn auto_discovers_local_overlay_alongside_root_manifest() {
        let _guard = LOCAL_OVERLAY_ENV_LOCK.lock().expect("env lock");
        std::env::remove_var("EFFIGY_NO_LOCAL_OVERLAY");
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[shell]
run = "root"
"#,
        );
        write_manifest(
            dir,
            "effigy.local.toml",
            r#"
[isolation]
paths = ["b"]
"#,
        );
        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "isolation.paths");
        assert_eq!(domains, vec!["b".to_owned()]);
        // Auto-discovered include shows up in the include graph.
        assert_eq!(loaded.include_graph.len(), 1);
        assert_eq!(
            loaded.include_graph[0].child.file_name(),
            Some(std::ffi::OsStr::new("effigy.local.toml"))
        );
    }

    #[test]
    fn no_local_overlay_env_disables_auto_discovery() {
        let _guard = LOCAL_OVERLAY_ENV_LOCK.lock().expect("env lock");
        std::env::set_var("EFFIGY_NO_LOCAL_OVERLAY", "1");
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[shell]
run = "root"
"#,
        );
        write_manifest(
            dir,
            "effigy.local.toml",
            r#"
[isolation]
paths = ["b"]
"#,
        );
        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        std::env::remove_var("EFFIGY_NO_LOCAL_OVERLAY");
        let isolation = loaded
            .effective_value
            .as_table()
            .and_then(|t| t.get("isolation"));
        assert!(
            isolation.is_none(),
            "local overlay should not have been merged: {:?}",
            isolation
        );
        assert!(loaded.include_graph.is_empty());
    }

    #[test]
    fn explicit_local_overlay_include_is_not_double_merged() {
        let _guard = LOCAL_OVERLAY_ENV_LOCK.lock().expect("env lock");
        std::env::remove_var("EFFIGY_NO_LOCAL_OVERLAY");
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "effigy.local.toml", optional = true, extend = ["isolation.paths"] },
]

[isolation]
paths = ["a"]
"#,
        );
        write_manifest(
            dir,
            "effigy.local.toml",
            r#"
[isolation]
paths = ["b"]
"#,
        );
        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "isolation.paths");
        // If double-merged, "b" would appear twice.
        assert_eq!(domains, vec!["a".to_owned(), "b".to_owned()]);
        let local_includes = loaded
            .include_graph
            .iter()
            .filter(|edge| {
                edge.child.file_name() == Some(std::ffi::OsStr::new("effigy.local.toml"))
            })
            .count();
        assert_eq!(local_includes, 1);
    }

    #[test]
    fn extend_passes_through_table_traversal() {
        // The path being extended is nested; merge must descend through tables
        // to reach the target array, not error at intermediate table merges.
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml", extend = ["scan.god_files.include"] },
]

[scan.god_files]
warn = 500
include = ["src/**"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[scan.god_files]
include = ["overlay/**"]
"#,
        );

        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "scan.god_files.include");
        assert_eq!(domains, vec!["src/**".to_owned(), "overlay/**".to_owned()]);
    }
}
