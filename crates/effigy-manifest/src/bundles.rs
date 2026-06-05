use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::manifest_section::{validate_minimum_effigy_version, ManifestSectionConfig};
use crate::ManifestError;
use crate::TASK_MANIFEST_FILE;

mod source;

pub use source::{
    inspect_bundle_source, sync_bundle_source, BundleSourceInspectReport, BundleSyncReport,
};
use source::{
    resolve_bundle_selection, resolve_materialized_bundle_source, BundleSelection,
    ResolvedBundleSource,
};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BundleSpec {
    pub name: String,
    pub description: String,
    pub inputs: Vec<BundleInputSpec>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BundleInputSpec {
    pub name: String,
    pub value_type: BundleInputType,
    pub required: bool,
    pub description: String,
    pub default: Option<Value>,
    pub example: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleInputType {
    String,
    Integer,
    Bool,
    List,
}

// The remote-source variants are introduced here so later git/OCI batches can
// widen the same source seam without another model break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BundleSourceType {
    Path,
    Git,
    Oci,
}

pub(crate) fn apply_bundle_defaults(
    manifest_path: &Path,
    current: &mut Value,
    extend_paths: &[String],
) -> Result<Option<AppliedBundleDefaults>, ManifestError> {
    let Some(bundle): Option<crate::config_sections::ManifestBundleConfig> = current
        .as_table()
        .and_then(|table| table.get("bundle"))
        .cloned()
        .map(|value| {
            value.try_into().map_err(|error| ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("invalid `[bundle]` section: {error}"),
            })
        })
        .transpose()?
    else {
        return Ok(None);
    };

    let selection = resolve_bundle_selection(manifest_path, &bundle)?;
    let normalized_inputs = bundle.inputs.clone();
    let resolved_source = resolve_materialized_bundle_source(manifest_path, &selection)?;
    let (mut defaults, source_path) = resolve_bundle_defaults_from_source(
        manifest_path,
        current,
        &selection,
        &resolved_source,
        &normalized_inputs,
    )?;
    let bundle_extend_paths = take_bundle_extend_paths(manifest_path, &mut defaults)?;
    let existing_extend_paths = combined_bundle_extend_paths(extend_paths, &bundle_extend_paths);
    let existing_bundle_values = existing_extend_paths
        .iter()
        .map(|path| (path.clone(), lookup_value_at_path(current, path).is_some()))
        .collect::<BTreeMap<_, _>>();
    merge_missing_values(current, &defaults);
    apply_bundle_extend_paths(
        manifest_path,
        current,
        &defaults,
        &existing_extend_paths,
        &existing_bundle_values,
    )?;
    Ok(Some(AppliedBundleDefaults {
        source_path,
        bundle_root: resolved_source.local_path,
    }))
}

fn resolve_bundle_defaults_from_source(
    manifest_path: &Path,
    _current: &Value,
    selection: &BundleSelection,
    source: &ResolvedBundleSource,
    normalized_inputs: &BTreeMap<String, Value>,
) -> Result<(Value, PathBuf), ManifestError> {
    match (selection, source.source_type) {
        (BundleSelection::Path { .. }, BundleSourceType::Path)
        | (BundleSelection::Git { .. }, BundleSourceType::Git)
        | (BundleSelection::Oci { .. }, BundleSourceType::Oci) => {
            resolve_local_bundle_defaults(manifest_path, &source.local_path, normalized_inputs)
        }
        _ => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "bundle source resolution mismatch between selection and materialization"
                .to_owned(),
        }),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AppliedBundleDefaults {
    pub source_path: PathBuf,
    pub bundle_root: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalBundleDescriptor {
    bundle: LocalBundleMetadata,
    #[serde(default)]
    inputs: Vec<LocalBundleInputDescriptor>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalBundleMetadata {
    name: String,
    #[serde(default, rename = "description")]
    _description: String,
    #[serde(default = "default_local_bundle_defaults_file")]
    defaults: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LocalBundleInputDescriptor {
    name: String,
    #[serde(rename = "type")]
    value_type: BundleInputType,
    #[serde(default)]
    required: bool,
    #[serde(default, rename = "description")]
    _description: String,
    #[serde(default)]
    default: Option<Value>,
    #[serde(default)]
    example: Option<Value>,
}

fn default_local_bundle_defaults_file() -> String {
    TASK_MANIFEST_FILE.to_owned()
}

pub(super) fn parse_bundle_descriptor_source(
    path: &Path,
    source: &str,
) -> Result<LocalBundleDescriptor, ManifestError> {
    toml::from_str::<LocalBundleDescriptor>(source).map_err(|error| ManifestError::Parse {
        path: path.to_path_buf(),
        error,
    })
}

fn resolve_local_bundle_defaults(
    manifest_path: &Path,
    bundle_dir: &Path,
    inputs: &BTreeMap<String, Value>,
) -> Result<(Value, PathBuf), ManifestError> {
    if !bundle_dir.is_dir() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "`[bundle].base = {{ type = \"path\", dir = ... }}` must point at a directory, got {}",
                bundle_dir.display()
            ),
        });
    }

    let descriptor_path = bundle_dir.join("bundle.toml");
    let descriptor_source =
        std::fs::read_to_string(&descriptor_path).map_err(|error| ManifestError::Read {
            path: descriptor_path.clone(),
            error,
        })?;
    let descriptor = parse_bundle_descriptor_source(&descriptor_path, &descriptor_source)?;
    validate_local_bundle_descriptor(manifest_path, &descriptor)?;
    let resolved_inputs = resolve_local_bundle_inputs(manifest_path, &descriptor, inputs)?;

    let defaults_path = bundle_dir.join(&descriptor.bundle.defaults);
    let defaults_template =
        std::fs::read_to_string(&defaults_path).map_err(|error| ManifestError::Read {
            path: defaults_path.clone(),
            error,
        })?;
    let rendered = render_bundle_template_with_inputs(
        manifest_path,
        &descriptor.bundle.name,
        bundle_dir,
        &defaults_template,
        &resolved_inputs,
    )?;
    let defaults = toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: defaults_path,
        error,
    })?;
    Ok((defaults, bundle_dir.join(&descriptor.bundle.defaults)))
}

fn validate_local_bundle_descriptor(
    manifest_path: &Path,
    descriptor: &LocalBundleDescriptor,
) -> Result<(), ManifestError> {
    let name = descriptor.bundle.name.trim();
    if name.is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "local bundle `bundle.name` must not be empty".to_owned(),
        });
    }
    if descriptor.bundle.defaults.trim().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("local bundle `{name}` `bundle.defaults` must not be empty"),
        });
    }

    let mut seen = std::collections::BTreeSet::new();
    for input in &descriptor.inputs {
        let input_name = input.name.trim();
        if input_name.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("local bundle `{name}` has an empty input name"),
            });
        }
        if matches!(input_name, "base" | "name" | "base_path") {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "local bundle `{name}` input `{input_name}` collides with a reserved `[bundle]` selector key"
                ),
            });
        }
        if !seen.insert(input_name.to_owned()) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "local bundle `{name}` declares input `{input_name}` more than once"
                ),
            });
        }
        if let Some(default) = &input.default {
            validate_bundle_input_type(manifest_path, name, input_name, input.value_type, default)?;
        }
        if let Some(example) = &input.example {
            validate_bundle_input_type(manifest_path, name, input_name, input.value_type, example)?;
        }
    }
    Ok(())
}

fn resolve_local_bundle_inputs(
    manifest_path: &Path,
    descriptor: &LocalBundleDescriptor,
    provided: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, ManifestError> {
    let bundle_name = descriptor.bundle.name.trim();
    let declared = descriptor
        .inputs
        .iter()
        .map(|input| (input.name.as_str(), input))
        .collect::<BTreeMap<_, _>>();
    for key in bundle_input_paths(provided) {
        if !declared.contains_key(key.as_str()) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("local bundle `{bundle_name}` does not declare input `{key}`"),
            });
        }
    }

    let mut resolved = BTreeMap::new();
    for input in &descriptor.inputs {
        let key = input.name.as_str();
        let value = bundle_input_value(provided, key)
            .cloned()
            .or_else(|| input.default.clone());
        match (value, input.required) {
            (Some(value), _) => {
                validate_bundle_input_type(
                    manifest_path,
                    bundle_name,
                    key,
                    input.value_type,
                    &value,
                )?;
                insert_bundle_input_value(&mut resolved, key, value);
            }
            (None, true) => {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("local bundle `{bundle_name}` requires input `{key}`"),
                });
            }
            (None, false) => {
                if input.value_type == BundleInputType::String {
                    insert_bundle_input_value(&mut resolved, key, Value::String(String::new()));
                }
            }
        }
    }
    normalize_database_bundle_inputs(manifest_path, bundle_name, &mut resolved)?;
    Ok(resolved)
}

fn normalize_database_bundle_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &mut BTreeMap<String, Value>,
) -> Result<(), ManifestError> {
    if inputs.contains_key("database") {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` input `database` has been removed; use `databases = [\"app\"]` instead"
            ),
        });
    }

    let Some(databases) =
        normalize_database_value(manifest_path, bundle_name, "databases", inputs)?
    else {
        return Ok(());
    };

    if !inputs.contains_key("databases") {
        inputs.insert("databases".to_owned(), Value::Array(databases.clone()));
    }
    let Some(primary) = databases.first().and_then(|value| value.as_str()) else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` normalized `databases` but found no primary database entry"
            ),
        });
    };
    inputs.insert("database".to_owned(), Value::String(primary.to_owned()));
    Ok(())
}

fn normalize_database_value(
    manifest_path: &Path,
    bundle_name: &str,
    field_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Option<Vec<Value>>, ManifestError> {
    match inputs.get("databases") {
        Some(Value::Array(values)) => {
            if values.is_empty() {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("bundle `{bundle_name}` input `{field_name}` must contain at least one database name"),
                });
            }
            let mut normalized = Vec::with_capacity(values.len());
            for value in values {
                let Some(name) = value
                    .as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return Err(ManifestError::Compose {
                        path: manifest_path.to_path_buf(),
                        detail: format!("bundle `{bundle_name}` input `{field_name}` must be a list of non-empty strings"),
                    });
                };
                normalized.push(Value::String(name.to_owned()));
            }
            Ok(Some(normalized))
        }
        Some(_) => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` input `{field_name}` must be a list of non-empty strings"
            ),
        }),
        None => Ok(None),
    }
}

pub(super) fn render_bundle_template_with_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    bundle_root: &Path,
    template: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<String, ManifestError> {
    let mut env = minijinja::Environment::new();
    env.add_function("bundle_host_label", render_bundle_host_label);
    env.add_function("bundle_host_path", render_bundle_host_path);
    env.add_function("bundle_workspace_subdir", render_bundle_workspace_subdir);
    env.add_function(
        "bundle_default_project_name",
        render_bundle_default_project_name,
    );
    env.add_function("bundle_validated_port", render_bundle_validated_port);
    env.add_function("route_domain", render_route_domain);
    env.add_template("bundle", template)
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template parse error: {error}"),
        })?;
    let template = env
        .get_template("bundle")
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template load error: {error}"),
        })?;
    let bundle_root = bundle_root.display().to_string();
    let manifest_root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .display()
        .to_string();
    let context = LocalBundleTemplateContext {
        inputs,
        bundle: LocalBundleTemplateBundle {
            name: bundle_name,
            root: &bundle_root,
            manifest_root: &manifest_root,
        },
    };
    template
        .render(context)
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template render error: {error}"),
        })
}

#[derive(serde::Serialize)]
struct LocalBundleTemplateContext<'a> {
    inputs: &'a BTreeMap<String, Value>,
    bundle: LocalBundleTemplateBundle<'a>,
}

#[derive(serde::Serialize)]
pub(super) struct LocalBundleTemplateBundle<'a> {
    pub(super) name: &'a str,
    pub(super) root: &'a str,
    pub(super) manifest_root: &'a str,
}

fn derive_bundle_workspace_subdir_from_roots(
    manifest_root: &Path,
    shared_root: &Path,
) -> Result<String, String> {
    let manifest_root = manifest_root
        .canonicalize()
        .unwrap_or_else(|_| manifest_root.to_path_buf());
    let shared_root = shared_root
        .canonicalize()
        .unwrap_or_else(|_| shared_root.to_path_buf());
    let relative = manifest_root.strip_prefix(&shared_root).map_err(|_| {
        format!(
            "bundle workspace subdir derivation requires repo root {} to be under shared_root {}",
            manifest_root.display(),
            shared_root.display()
        )
    })?;
    if relative.as_os_str().is_empty() {
        return Err(
            "bundle workspace subdir derivation requires `workspace_subdir` when the repo root equals `shared_root`"
                .to_owned(),
        );
    }
    Ok(relative.display().to_string())
}

fn derive_host_label(host: &str) -> Result<String, String> {
    let trimmed = host.trim().trim_end_matches('.');
    let Some(first_label) = trimmed.split('.').next() else {
        return Err("bundle host label derivation requires a non-empty `host`".to_owned());
    };
    if first_label.is_empty() {
        return Err(format!(
            "bundle host label derivation requires a non-empty first label in `host = {host}`"
        ));
    }
    Ok(first_label.to_owned())
}

pub(super) fn resolve_bundle_host_path(manifest_path: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    }
}

fn render_route_domain(host: String, label: Option<String>, fallback: Option<String>) -> String {
    route_domain_with_fallback(&host, label.as_deref(), fallback.as_deref())
}

fn render_bundle_host_label(host: String) -> Result<String, minijinja::Error> {
    derive_host_label(&host)
        .map_err(|detail| minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, detail))
}

fn render_bundle_host_path(manifest_root: String, path: String) -> String {
    let resolved = resolve_bundle_host_path(
        Path::new(&manifest_root).join(TASK_MANIFEST_FILE).as_path(),
        &path,
    );
    resolved
        .canonicalize()
        .unwrap_or(resolved)
        .display()
        .to_string()
}

fn render_bundle_workspace_subdir(
    manifest_root: String,
    shared_root: String,
) -> Result<String, minijinja::Error> {
    let shared_root = resolve_bundle_host_path(
        Path::new(&manifest_root).join(TASK_MANIFEST_FILE).as_path(),
        &shared_root,
    );
    derive_bundle_workspace_subdir_from_roots(Path::new(&manifest_root), &shared_root)
        .map_err(|detail| minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, detail))
}

fn render_bundle_default_project_name(prefix: String, workspace_subdir: String) -> String {
    default_project_name_from_workspace_subdir(&prefix, &workspace_subdir)
}

fn render_bundle_validated_port(value: i64, input_name: String) -> Result<i64, minijinja::Error> {
    if value <= 0 || value > u16::MAX as i64 {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "invalid bundle input `{input_name} = {value}`; expected a port in the range 1-65535"
            ),
        ))
    } else {
        Ok(value)
    }
}

fn route_domain_with_fallback(host: &str, label: Option<&str>, fallback: Option<&str>) -> String {
    let label = label
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| fallback.map(str::trim).filter(|value| !value.is_empty()))
        .unwrap_or_default();
    if label.is_empty() {
        host.to_owned()
    } else {
        format!("{label}.{host}")
    }
}

pub(super) fn default_project_name_from_workspace_subdir(
    prefix: &str,
    workspace_subdir: &str,
) -> String {
    let slug = workspace_subdir
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        format!("{prefix}-dev")
    } else {
        format!("{prefix}-{slug}-dev")
    }
}

pub(super) fn bundle_input_value<'a>(
    inputs: &'a BTreeMap<String, Value>,
    key: &str,
) -> Option<&'a Value> {
    let mut segments = key.split('.');
    let first = segments.next()?;
    let mut current = inputs.get(first)?;
    for segment in segments {
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

pub(super) fn bundle_input_paths(inputs: &BTreeMap<String, Value>) -> Vec<String> {
    let mut paths = Vec::new();
    for (key, value) in inputs {
        collect_bundle_input_paths(key, value, &mut paths);
    }
    paths
}

fn collect_bundle_input_paths(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Table(table) => {
            for (key, child) in table {
                let child_prefix = if prefix.is_empty() {
                    key.to_owned()
                } else {
                    format!("{prefix}.{key}")
                };
                collect_bundle_input_paths(&child_prefix, child, out);
            }
        }
        _ => out.push(prefix.to_owned()),
    }
}

pub(super) fn insert_bundle_input_value(
    inputs: &mut BTreeMap<String, Value>,
    key: &str,
    value: Value,
) {
    fn insert_nested_segments(
        table: &mut toml::map::Map<String, Value>,
        segments: &[&str],
        value: Value,
    ) {
        if let Some((head, tail)) = segments.split_first() {
            if tail.is_empty() {
                table.insert((*head).to_owned(), value);
                return;
            }
            let entry = table
                .entry((*head).to_owned())
                .or_insert_with(|| Value::Table(toml::map::Map::new()));
            let nested = entry
                .as_table_mut()
                .expect("bundle input path prefixes must be tables");
            insert_nested_segments(nested, tail, value);
        }
    }

    let segments = key.split('.').collect::<Vec<_>>();
    if let Some((head, tail)) = segments.split_first() {
        if tail.is_empty() {
            inputs.insert((*head).to_owned(), value);
            return;
        }
        let entry = inputs
            .entry((*head).to_owned())
            .or_insert_with(|| Value::Table(toml::map::Map::new()));
        let nested = entry
            .as_table_mut()
            .expect("bundle input path prefixes must be tables");
        insert_nested_segments(nested, tail, value);
    }
}

pub(super) fn validate_bundle_input_type(
    manifest_path: &Path,
    bundle_name: &str,
    key: &str,
    expected: BundleInputType,
    value: &Value,
) -> Result<(), ManifestError> {
    let ok = match expected {
        BundleInputType::String => value.is_str(),
        BundleInputType::Integer => value.is_integer(),
        BundleInputType::Bool => value.is_bool(),
        BundleInputType::List => value.is_array(),
    };
    if ok {
        return Ok(());
    }
    Err(ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: format!(
            "bundle `{bundle_name}` input `{key}` must be {}, got {}",
            bundle_input_type_name(expected),
            toml_type_name(value)
        ),
    })
}

fn bundle_input_type_name(value_type: BundleInputType) -> &'static str {
    match value_type {
        BundleInputType::String => "a string",
        BundleInputType::Integer => "an integer",
        BundleInputType::Bool => "a bool",
        BundleInputType::List => "a list",
    }
}

fn toml_type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Integer(_) => "integer",
        Value::Float(_) => "float",
        Value::Boolean(_) => "bool",
        Value::Datetime(_) => "datetime",
        Value::Array(_) => "list",
        Value::Table(_) => "table",
    }
}

pub(super) fn merge_missing_values(current: &mut Value, defaults: &Value) {
    if let (Some(current_table), Some(defaults_table)) =
        (current.as_table_mut(), defaults.as_table())
    {
        for (key, default_value) in defaults_table {
            match current_table.get_mut(key) {
                Some(current_value) => merge_missing_values(current_value, default_value),
                None => {
                    current_table.insert(key.clone(), default_value.clone());
                }
            }
        }
    }
}

fn take_bundle_extend_paths(
    manifest_path: &Path,
    defaults: &mut Value,
) -> Result<Vec<String>, ManifestError> {
    let Some(defaults_table) = defaults.as_table_mut() else {
        return Ok(Vec::new());
    };
    let Some(section) = defaults_table.remove("manifest") else {
        return Ok(Vec::new());
    };
    let config: ManifestSectionConfig =
        section.try_into().map_err(|error| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid bundle `[manifest]` section: {error}"),
        })?;
    validate_minimum_effigy_version(manifest_path, config.minimum_effigy_version.as_deref())?;
    Ok(config.extend)
}

pub(super) fn apply_bundle_extend_paths(
    manifest_path: &Path,
    current: &mut Value,
    defaults: &Value,
    extend_paths: &[String],
    existing_values: &BTreeMap<String, bool>,
) -> Result<(), ManifestError> {
    for path in extend_paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: "invalid `[bundle]` section: `extend[]` must not contain empty paths"
                    .to_owned(),
            });
        }
        if !existing_values.get(trimmed).copied().unwrap_or(false) {
            continue;
        }
        apply_bundle_extend_path(manifest_path, current, defaults, trimmed)?;
    }
    Ok(())
}

fn combined_bundle_extend_paths(base: &[String], incoming: &[String]) -> Vec<String> {
    let mut combined = base.to_vec();
    for path in incoming {
        if !combined.contains(path) {
            combined.push(path.clone());
        }
    }
    combined
}

fn apply_bundle_extend_path(
    manifest_path: &Path,
    current: &mut Value,
    defaults: &Value,
    path: &str,
) -> Result<(), ManifestError> {
    let Some(default_value) = lookup_value_at_path(defaults, path) else {
        return Ok(());
    };
    let Some(current_value) = lookup_value_at_path_mut(current, path) else {
        return Ok(());
    };
    if let (Some(default_array), Some(current_array)) =
        (default_value.as_array(), current_value.as_array_mut())
    {
        let mut combined = default_array.clone();
        combined.extend(current_array.iter().cloned());
        *current_array = combined;
        return Ok(());
    }
    if let (Some(default_table), Some(current_table)) =
        (default_value.as_table(), current_value.as_table())
    {
        let mut combined = Value::Table(default_table.clone());
        merge_values_with_incoming_overrides(&mut combined, &Value::Table(current_table.clone()));
        *current_value = combined;
        return Ok(());
    }
    Err(ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail: format!(
            "invalid `[bundle]` section: extend path `{path}` requires arrays or tables in both bundle defaults and the manifest"
        ),
    })
}

fn merge_values_with_incoming_overrides(current: &mut Value, incoming: &Value) {
    if let (Some(current_table), Some(incoming_table)) =
        (current.as_table_mut(), incoming.as_table())
    {
        for (key, incoming_value) in incoming_table {
            match current_table.get_mut(key) {
                Some(current_value) => {
                    merge_values_with_incoming_overrides(current_value, incoming_value)
                }
                None => {
                    current_table.insert(key.clone(), incoming_value.clone());
                }
            }
        }
    } else {
        *current = incoming.clone();
    }
}

pub(super) fn lookup_value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

pub(super) fn lookup_value_at_path_mut<'a>(
    value: &'a mut Value,
    path: &str,
) -> Option<&'a mut Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.as_table_mut()?.get_mut(segment)?;
    }
    Some(current)
}
