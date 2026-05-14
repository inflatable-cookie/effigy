use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use toml::Value;

use super::TaskManifest;
use crate::bundles::apply_bundle_defaults;
use crate::config_sections::ManifestBundleConfig;
use crate::manifest_section::{
    resolve_include_path, validate_minimum_effigy_version, ManifestIncludeEntry,
    ManifestSectionConfig,
};
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

#[derive(Debug, Clone)]
struct ManifestIncludeSpec {
    resolved_path: PathBuf,
    override_paths: Vec<String>,
    optional: bool,
}

#[derive(Debug, Clone)]
struct ComposedValue {
    value: Value,
    source_map: BTreeMap<String, PathBuf>,
    extend_paths: Vec<String>,
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
    let bundle_defaults =
        apply_bundle_defaults(manifest_path, &mut composed.value, &composed.extend_paths)?;
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

pub(crate) fn load_manifest_bundle_config(
    manifest_path: &Path,
) -> Result<Option<ManifestBundleConfig>, ManifestError> {
    let mut session = CompositionSession::default();
    let composed = load_composed_value(manifest_path, &mut session)?;
    composed
        .value
        .as_table()
        .and_then(|table| table.get("bundle"))
        .cloned()
        .map(|value| {
            value.try_into().map_err(|error| ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("invalid `[bundle]` section: {error}"),
            })
        })
        .transpose()
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

    let (mut includes, manifest_extend_paths) = take_include_specs(manifest_path, &mut table)?;
    if session.stack.len() == 1 {
        append_local_overlay_include(manifest_path, &mut includes);
    }
    let mut composed = ComposedValue {
        value: Value::Table(table),
        source_map: BTreeMap::new(),
        extend_paths: manifest_extend_paths,
    };
    record_value_sources("", &composed.value, manifest_path, &mut composed.source_map);

    for include in includes {
        if include.optional && !include.resolved_path.exists() {
            continue;
        }
        let child = load_composed_value(&include.resolved_path, session)?;
        for path in &child.extend_paths {
            if include.override_paths.contains(path) {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "include `{}` declares `{}` in `override`, but the imported fragment declares it in `[manifest].extend`; pick one",
                        include.resolved_path.display(),
                        path
                    ),
                });
            }
        }
        session.include_graph.push(ManifestCompositionEdge {
            parent: manifest_path.to_path_buf(),
            child: include.resolved_path.clone(),
            override_paths: include.override_paths.clone(),
            extend_paths: child.extend_paths.clone(),
        });
        merge_values(
            "",
            &mut composed.value,
            &child.value,
            &child.source_map,
            &include,
            &child.extend_paths,
            &mut session.overridden_paths,
            &mut composed.source_map,
            manifest_path,
        )?;
        for path in child.extend_paths {
            if !composed.extend_paths.contains(&path) {
                composed.extend_paths.push(path);
            }
        }
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
) -> Result<(Vec<ManifestIncludeSpec>, Vec<String>), ManifestError> {
    let Some(section) = table.remove("manifest") else {
        return Ok((Vec::new(), Vec::new()));
    };
    let config: ManifestSectionConfig =
        section.try_into().map_err(|error| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid `[manifest]` section: {error}"),
        })?;
    validate_minimum_effigy_version(manifest_path, config.minimum_effigy_version.as_deref())?;
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut specs = Vec::with_capacity(config.include.len());
    for entry in config.include {
        let (path, override_paths, optional) = match entry {
            ManifestIncludeEntry::Path(path) => (path, Vec::new(), false),
            ManifestIncludeEntry::Detailed(detail) => {
                (detail.path, detail.override_paths, detail.optional)
            }
        };
        let resolved_path = resolve_include_path(parent, &path);
        specs.push(ManifestIncludeSpec {
            resolved_path,
            override_paths,
            optional,
        });
    }
    Ok((specs, config.extend))
}

fn merge_values(
    path: &str,
    current: &mut Value,
    incoming: &Value,
    incoming_sources: &BTreeMap<String, PathBuf>,
    include: &ManifestIncludeSpec,
    extend_paths: &[String],
    overridden_paths: &mut Vec<ManifestCompositionOverride>,
    current_sources: &mut BTreeMap<String, PathBuf>,
    root_manifest_path: &Path,
) -> Result<(), ManifestError> {
    let override_set = include
        .override_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let extend_set = extend_paths.iter().cloned().collect::<BTreeSet<_>>();
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
        if let (Some(current_array), Some(incoming_array)) =
            (current.as_array_mut(), incoming.as_array())
        {
            current_array.extend(incoming_array.iter().cloned());
            used_extends.insert(path.to_owned());
            return Ok(());
        }
        if current.is_table() && incoming.is_table() {
            merge_extended_table(
                path,
                current,
                incoming,
                incoming_sources,
                current_sources,
                incoming_fragment,
            );
            used_extends.insert(path.to_owned());
            return Ok(());
        }
        return Err(ManifestError::Compose {
            path: root_manifest_path.to_path_buf(),
            detail: format!(
                "extend path `{path}` requires arrays or tables on both sides; got {} from {} and {} from {}",
                value_kind(current),
                existing_source.display(),
                value_kind(incoming),
                incoming_fragment.display()
            ),
        });
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

fn merge_extended_table(
    path: &str,
    current: &mut Value,
    incoming: &Value,
    incoming_sources: &BTreeMap<String, PathBuf>,
    current_sources: &mut BTreeMap<String, PathBuf>,
    incoming_fragment: &Path,
) {
    let (Some(current_table), Some(incoming_table)) = (current.as_table_mut(), incoming.as_table())
    else {
        *current = incoming.clone();
        remove_source_entries(path, current_sources);
        copy_source_entries(
            path,
            incoming_sources,
            current_sources,
            incoming_fragment,
            incoming,
        );
        return;
    };

    for (key, incoming_value) in incoming_table {
        let child_path = join_path(path, key);
        match current_table.get_mut(key) {
            Some(current_value) => {
                if current_value.is_table() && incoming_value.is_table() {
                    merge_extended_table(
                        &child_path,
                        current_value,
                        incoming_value,
                        incoming_sources,
                        current_sources,
                        incoming_fragment,
                    );
                } else {
                    *current_value = incoming_value.clone();
                    remove_source_entries(&child_path, current_sources);
                    copy_source_entries(
                        &child_path,
                        incoming_sources,
                        current_sources,
                        incoming_fragment,
                        incoming_value,
                    );
                }
            }
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
    use semver::Version;
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
include = ["overlay.toml"]

[isolation]
paths = ["a", "b"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["isolation.paths"]

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
    fn child_manifest_extend_appends_without_parent_include_directive() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = ["overlay.toml"]

[isolation]
paths = ["a", "b"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["isolation.paths"]

[isolation]
paths = ["c", "d"]
"#,
        );

        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let paths = array_strings(&loaded.effective_value, "isolation.paths");
        assert_eq!(
            paths,
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
            ]
        );
    }

    #[test]
    fn extend_merges_table_entries() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = ["overlay.toml"]

[secrets]
backend = "external"

[secrets.keys.local_token]
required = true
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["secrets"]

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.shared_token]
required = false
"#,
        );

        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let secrets = loaded
            .effective_value
            .as_table()
            .and_then(|table| table.get("secrets"))
            .and_then(Value::as_table)
            .expect("secrets table");
        assert_eq!(
            secrets.get("backend").and_then(Value::as_str),
            Some("effigy-vault")
        );
        assert!(secrets
            .get("vault")
            .and_then(Value::as_table)
            .and_then(|table| table.get("path"))
            .and_then(Value::as_str)
            .is_some());
        let keys = secrets.get("keys").and_then(Value::as_table).expect("keys");
        assert!(keys.contains_key("local_token"));
        assert!(keys.contains_key("shared_token"));
    }

    #[test]
    fn extend_on_scalar_path_errors() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = ["overlay.toml"]

[shell]
run = "primary"
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["shell.run"]

[shell]
run = "secondary"
"#,
        );

        let err = load_task_manifest_with_inspection(&root).unwrap_err();
        let detail = format!("{err}");
        assert!(detail.contains("extend path `shell.run`"), "{detail}");
        assert!(
            detail.contains("requires arrays or tables on both sides"),
            "{detail}"
        );
        assert!(detail.contains("effigy.toml"), "{detail}");
        assert!(detail.contains("overlay.toml"), "{detail}");
    }

    #[test]
    fn include_side_extend_is_rejected() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = [
  { path = "overlay.toml",
    extend = ["isolation.paths"] },
]
"#,
        );
        let err = load_task_manifest_with_inspection(&root).unwrap_err();
        let detail = format!("{err}");
        assert!(detail.contains("invalid `[manifest]` section"), "{detail}");
        assert!(detail.contains("ManifestIncludeEntry"), "{detail}");
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
include = ["overlay.toml"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["isolation.paths"]

[isolation]
paths = ["a"]
"#,
        );

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
  { path = "overlay.toml", optional = true },
]

[isolation]
paths = ["a"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["isolation.paths"]

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
  { path = "effigy.local.toml", optional = true },
]

[isolation]
paths = ["a"]
"#,
        );
        write_manifest(
            dir,
            "effigy.local.toml",
            r#"
[manifest]
extend = ["isolation.paths"]

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
include = ["overlay.toml"]

[scan.god_files]
warn = 500
include = ["src/**"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
extend = ["scan.god_files.include"]

[scan.god_files]
include = ["overlay/**"]
"#,
        );

        let loaded = load_task_manifest_with_inspection(&root).expect("load");
        let domains = array_strings(&loaded.effective_value, "scan.god_files.include");
        assert_eq!(domains, vec!["src/**".to_owned(), "overlay/**".to_owned()]);
    }

    #[test]
    fn manifest_minimum_effigy_version_accepts_current_version() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            &format!(
                r#"
[manifest]
minimum_effigy_version = "{}"

[shell]
run = "ok"
"#,
                env!("CARGO_PKG_VERSION")
            ),
        );

        load_task_manifest_with_inspection(&root).expect("load");
    }

    #[test]
    fn manifest_minimum_effigy_version_rejects_newer_root_requirement() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
minimum_effigy_version = "999.0.0"
"#,
        );
        let result = load_task_manifest_with_inspection(&root);
        if effigy_core::build_info::active_version().contains("+local.") {
            result.expect("local dev build should bypass manifest floor");
        } else {
            let err = result.unwrap_err();
            let detail = err.to_string();
            assert!(detail.contains("requires Effigy >= 999.0.0"), "{detail}");
            assert!(detail.contains(env!("CARGO_PKG_VERSION")), "{detail}");
        }
    }

    #[test]
    fn manifest_minimum_effigy_version_rejects_newer_included_fragment_requirement() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
include = ["overlay.toml"]
"#,
        );
        write_manifest(
            dir,
            "overlay.toml",
            r#"
[manifest]
minimum_effigy_version = "999.0.0"
"#,
        );
        let result = load_task_manifest_with_inspection(&root);
        if effigy_core::build_info::active_version().contains("+local.") {
            result.expect("local dev build should bypass included manifest floor");
        } else {
            let err = result.unwrap_err();
            let detail = err.to_string();
            assert!(detail.contains("overlay.toml"), "{detail}");
            assert!(detail.contains("requires Effigy >= 999.0.0"), "{detail}");
        }
    }

    #[test]
    fn manifest_minimum_effigy_version_rejects_invalid_semver() {
        let tmp = tempdir().expect("tempdir");
        let dir = tmp.path();
        let root = write_manifest(
            dir,
            "effigy.toml",
            r#"
[manifest]
minimum_effigy_version = "latest"
"#,
        );

        let err = load_task_manifest_with_inspection(&root).unwrap_err();
        let detail = err.to_string();
        assert!(
            detail.contains("`[manifest].minimum_effigy_version` must be a valid semver version"),
            "{detail}"
        );
    }

    #[test]
    fn manifest_minimum_effigy_version_accepts_local_dev_builds_for_newer_floors() {
        let requested = Version::parse("0.7.0").expect("requested");
        assert!(crate::manifest_section::active_version_satisfies_minimum_effigy_version(
            "0.6.1+local.67a79ff.dirty",
            &requested,
        )
        .expect("local build should bypass floor"));
        assert!(crate::manifest_section::active_version_satisfies_minimum_effigy_version(
            "v0.6.1+local.67a79ff",
            &requested
        )
        .expect("prefixed local build should bypass floor"));
    }

    #[test]
    fn manifest_minimum_effigy_version_rejects_newer_floor_for_release_builds() {
        let requested = Version::parse("0.7.0").expect("requested");
        assert!(
            !crate::manifest_section::active_version_satisfies_minimum_effigy_version(
                "0.6.1",
                &requested
            )
                .expect("release build should stay strict")
        );
    }

    #[test]
    fn manifest_minimum_effigy_version_rejects_invalid_current_binary_version() {
        let requested = Version::parse("0.6.1").expect("requested");
        let err =
            crate::manifest_section::active_version_satisfies_minimum_effigy_version(
                "definitely-not-semver",
                &requested,
            )
            .expect_err("invalid active version should fail");
        assert!(err.contains("current Effigy version is invalid"), "{err}");
    }
}
