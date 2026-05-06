use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use crate::ManifestError;

mod export;
mod specs;

use export::{materialize_shipped_bundle_assets, shipped_bundle_export_files};
use specs::{
    decodelabs_library_spec, decodelabs_spec, resolve_decodelabs_bundle,
    resolve_decodelabs_library_bundle, resolve_underlay_bundle, underlay_spec,
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BundleExport {
    pub bundle: String,
    pub path: PathBuf,
    pub files: Vec<String>,
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
    let mut normalized_inputs = bundle.inputs.clone();
    let bundle_name = match &selection {
        BundleSelection::Shipped { name } => name.as_str(),
        BundleSelection::Local { .. } => "",
    };
    if !bundle_name.is_empty() {
        normalize_database_bundle_inputs(manifest_path, bundle_name, &mut normalized_inputs)?;
        normalize_bundle_specific_inputs(manifest_path, bundle_name, &mut normalized_inputs)?;
    }
    let (mut defaults, source_path) = match &selection {
        BundleSelection::Shipped { name } => (
            resolve_bundle_defaults(manifest_path, current, name, &normalized_inputs)?,
            bundle_source_path(name),
        ),
        BundleSelection::Local { path } => {
            resolve_local_bundle_defaults(manifest_path, path, &normalized_inputs)?
        }
    };
    let bundle_extend_paths = take_bundle_extend_paths(manifest_path, &mut defaults)?;
    let existing_extend_paths = combined_bundle_extend_paths(extend_paths, &bundle_extend_paths);
    let existing_bundle_values = existing_extend_paths
        .iter()
        .map(|path| (path.clone(), lookup_value_at_path(current, path).is_some()))
        .collect::<BTreeMap<_, _>>();
    let bundle_root = match &selection {
        BundleSelection::Shipped { name } => {
            materialize_shipped_bundle_assets(manifest_path, name)?
        }
        BundleSelection::Local { path } => path.clone(),
    };
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
        bundle_root,
    }))
}

pub(crate) fn bundle_source_path(name: &str) -> PathBuf {
    PathBuf::from(format!("<bundle:{name}>"))
}

#[derive(Debug, Clone)]
pub(crate) struct AppliedBundleDefaults {
    pub source_path: PathBuf,
    pub bundle_root: PathBuf,
}

enum BundleSelection {
    Shipped { name: String },
    Local { path: PathBuf },
}

fn resolve_bundle_selection(
    manifest_path: &Path,
    bundle: &crate::config_sections::ManifestBundleConfig,
) -> Result<BundleSelection, ManifestError> {
    match (bundle.base.as_deref(), bundle.base_path.as_deref()) {
        (Some(_), Some(_)) => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "`[bundle]` cannot set both `base` and `base_path`".to_owned(),
        }),
        (Some(base), None) if !base.trim().is_empty() => Ok(BundleSelection::Shipped {
            name: base.trim().to_owned(),
        }),
        (None, Some(path)) if !path.trim().is_empty() => {
            let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
            let path = Path::new(path.trim());
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                manifest_dir.join(path)
            };
            Ok(BundleSelection::Local { path: resolved })
        }
        _ => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "`[bundle]` must set either `base` for a shipped bundle or `base_path` for a local bundle directory".to_owned(),
        }),
    }
}

pub fn list_bundles() -> Vec<BundleSpec> {
    vec![
        decodelabs_spec(),
        decodelabs_library_spec(),
        underlay_spec(),
    ]
}

pub fn get_bundle(name: &str) -> Option<BundleSpec> {
    list_bundles()
        .into_iter()
        .find(|bundle| bundle.name == name)
}

pub fn render_bundle_defaults(
    name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    let mut normalized_inputs = inputs.clone();
    normalize_database_bundle_inputs(&bundle_source_path(name), name, &mut normalized_inputs)?;
    normalize_bundle_specific_inputs(&bundle_source_path(name), name, &mut normalized_inputs)?;
    resolve_bundle_defaults(
        &bundle_source_path(name),
        &Value::Table(Default::default()),
        name,
        &normalized_inputs,
    )
}

pub fn list_bundle_default_paths(name: &str) -> Result<Vec<String>, ManifestError> {
    let spec = get_bundle(name).ok_or_else(|| ManifestError::Compose {
        path: bundle_source_path(name),
        detail: format!("unknown bundle `{name}`"),
    })?;
    let example_inputs = spec
        .inputs
        .iter()
        .map(|input| {
            (
                input.name.clone(),
                input
                    .default
                    .clone()
                    .or_else(|| input.example.clone())
                    .unwrap_or_else(|| Value::String(format!("<{}>", input.name))),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let defaults = render_bundle_defaults(name, &example_inputs)?;
    let mut paths = Vec::new();
    collect_value_paths("", &defaults, &mut paths);
    Ok(paths)
}

pub fn export_bundle(name: &str, target_dir: &Path) -> Result<BundleExport, ManifestError> {
    let files = shipped_bundle_export_files(name)?;
    if target_dir.exists() && !target_dir.is_dir() {
        return Err(ManifestError::Compose {
            path: target_dir.to_path_buf(),
            detail: "bundle export path exists but is not a directory".to_owned(),
        });
    }
    std::fs::create_dir_all(target_dir).map_err(|error| ManifestError::Read {
        path: target_dir.to_path_buf(),
        error,
    })?;

    for file in &files {
        let path = target_dir.join(file.path);
        if path.exists() {
            return Err(ManifestError::Compose {
                path,
                detail:
                    "bundle export refuses to overwrite existing files; choose an empty directory"
                        .to_owned(),
            });
        }
    }

    let mut written = Vec::new();
    for file in files {
        let path = target_dir.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        std::fs::write(&path, file.contents).map_err(|error| ManifestError::Read {
            path: path.clone(),
            error,
        })?;
        written.push(file.path.to_owned());
    }

    Ok(BundleExport {
        bundle: name.to_owned(),
        path: target_dir.to_path_buf(),
        files: written,
    })
}

fn resolve_bundle_defaults(
    manifest_path: &Path,
    current: &Value,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    match bundle_name {
        "decodelabs" => resolve_decodelabs_bundle(manifest_path, inputs),
        "decodelabs-library" => resolve_decodelabs_library_bundle(manifest_path, inputs),
        "underlay" => resolve_underlay_bundle(manifest_path, current, inputs),
        other => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("unknown bundle `{other}`"),
        }),
    }
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
    "effigy.toml".to_owned()
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct BundleManifestSectionConfig {
    #[serde(default)]
    extend: Vec<String>,
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

pub(super) fn bundle_spec_from_descriptor(descriptor: &LocalBundleDescriptor) -> BundleSpec {
    BundleSpec {
        name: descriptor.bundle.name.clone(),
        description: descriptor.bundle._description.clone(),
        inputs: descriptor
            .inputs
            .iter()
            .map(|input| BundleInputSpec {
                name: input.name.clone(),
                value_type: input.value_type,
                required: input.required,
                description: input._description.clone(),
                default: input.default.clone(),
                example: input.example.clone(),
            })
            .collect(),
    }
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
                "`[bundle].base_path` must point at a directory, got {}",
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
        if matches!(input_name, "base" | "base_path") {
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
            (None, false) => {}
        }
    }
    normalize_database_bundle_inputs(manifest_path, bundle_name, &mut resolved)?;
    normalize_bundle_specific_inputs(manifest_path, bundle_name, &mut resolved)?;
    Ok(resolved)
}

fn normalize_bundle_specific_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &mut BTreeMap<String, Value>,
) -> Result<(), ManifestError> {
    if bundle_name == "underlay" {
        ensure_optional_bundle_string_inputs(
            inputs,
            &[
                "dirs.docs",
                "dirs.api",
                "dirs.client",
                "dirs.ui",
                "dirs.front",
                "dirs.admin",
                "routes.front",
                "routes.admin",
                "routes.api",
                "sources.underlay",
                "sources.poodle",
            ],
        );
        let host = required_bundle_string(manifest_path, bundle_name, inputs, "host")?;
        for (output, input, default_label) in [
            ("front_route_domain", "routes.front", None),
            ("admin_route_domain", "routes.admin", Some("admin")),
            ("api_route_domain", "routes.api", Some("api")),
        ] {
            insert_bundle_input_value(
                inputs,
                output,
                Value::String(underlay_route_domain(
                    &host,
                    optional_bundle_string(inputs, input)
                        .as_deref()
                        .or(default_label),
                )),
            );
        }
        return Ok(());
    }

    if bundle_name == "decodelabs-library" {
        let shared_root_path = bundle_shared_root_path(manifest_path, bundle_name, inputs)?;
        inputs.insert(
            "shared_root".to_owned(),
            Value::String(shared_root_path.display().to_string()),
        );
        if !inputs.contains_key("workspace_subdir") {
            let workspace_subdir = derive_bundle_workspace_subdir(
                manifest_path,
                &shared_root_path.display().to_string(),
            )?;
            inputs.insert(
                "workspace_subdir".to_owned(),
                Value::String(workspace_subdir),
            );
        }
    }

    Ok(())
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

fn ensure_optional_bundle_string_inputs(inputs: &mut BTreeMap<String, Value>, keys: &[&str]) {
    for key in keys {
        if bundle_input_value(inputs, key).is_none() {
            insert_bundle_input_value(inputs, key, Value::String(String::new()));
        }
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
    let context = LocalBundleTemplateContext {
        inputs,
        bundle: LocalBundleTemplateBundle {
            name: bundle_name,
            root: &bundle_root,
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
}

pub(super) fn required_bundle_string(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
    key: &str,
) -> Result<String, ManifestError> {
    let Some(value) = bundle_input_value(inputs, key) else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` requires string input `{key}`"),
        });
    };
    let Some(value) = value.as_str() else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` input `{key}` must be a string"),
        });
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` input `{key}` must not be empty"),
        });
    }
    Ok(value.to_owned())
}

pub(super) fn bundle_shared_root_input(
    _manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<String, ManifestError> {
    Ok(
        optional_bundle_string(inputs, "shared_root").unwrap_or_else(|| {
            bundle_default_input_string(bundle_name, "shared_root")
                .unwrap_or_else(|| "../".to_owned())
        }),
    )
}

pub(super) fn bundle_shared_root_path(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<PathBuf, ManifestError> {
    let shared_root = bundle_shared_root_input(manifest_path, bundle_name, inputs)?;
    let shared_root_path = resolve_bundle_host_path(manifest_path, &shared_root);
    Ok(shared_root_path.canonicalize().unwrap_or(shared_root_path))
}

pub(super) fn bundle_default_input_string(bundle_name: &str, key: &str) -> Option<String> {
    list_bundles()
        .into_iter()
        .find(|spec| spec.name == bundle_name)
        .and_then(|spec| spec.inputs.into_iter().find(|input| input.name == key))
        .and_then(|input| input.default)
        .and_then(|value| value.as_str().map(str::to_owned))
}

pub(super) fn optional_bundle_integer(inputs: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    bundle_input_value(inputs, key).and_then(Value::as_integer)
}

pub(super) fn optional_bundle_string(
    inputs: &BTreeMap<String, Value>,
    key: &str,
) -> Option<String> {
    let value = bundle_input_value(inputs, key)?.as_str()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

pub(super) fn render_toml_string_list(inputs: &BTreeMap<String, Value>, key: &str) -> String {
    let Some(values) = bundle_input_value(inputs, key).and_then(Value::as_array) else {
        return "[]".to_owned();
    };
    let encoded = values
        .iter()
        .filter_map(Value::as_str)
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{encoded}]")
}

pub(super) fn render_toml_string_array_lines(values: &[&str], indent: &str) -> String {
    values
        .iter()
        .map(|value| format!("{indent}{value:?},"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn derive_bundle_workspace_subdir(
    manifest_path: &Path,
    shared_root: &str,
) -> Result<String, ManifestError> {
    let manifest_root = manifest_path
        .parent()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "bundle workspace subdir derivation requires a manifest parent directory"
                .to_owned(),
        })?;
    let shared_root = resolve_bundle_host_path(manifest_path, shared_root);
    let manifest_root = manifest_root
        .canonicalize()
        .unwrap_or_else(|_| manifest_root.to_path_buf());
    let shared_root = shared_root.canonicalize().unwrap_or(shared_root);
    let relative = manifest_root
        .strip_prefix(&shared_root)
        .map_err(|_| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `decodelabs-library` could not derive `workspace_subdir` because repo root {} is not under shared_root {}",
                manifest_root.display(),
                shared_root.display()
            ),
        })?;
    if relative.as_os_str().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: "bundle `decodelabs-library` requires `workspace_subdir` when the repo root equals `shared_root`".to_owned(),
        });
    }
    Ok(relative.display().to_string())
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

pub(super) fn underlay_route_domain(host: &str, label: Option<&str>) -> String {
    let label = label.map(str::trim).unwrap_or_default();
    if label.is_empty() {
        host.to_owned()
    } else {
        format!("{label}.{host}")
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
    let config: BundleManifestSectionConfig =
        section.try_into().map_err(|error| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid bundle `[manifest]` section: {error}"),
        })?;
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
    let default_array = default_value
        .as_array()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
            "invalid `[bundle]` section: extend path `{path}` requires an array in bundle defaults"
        ),
        })?;
    let current_array = current_value
        .as_array_mut()
        .ok_or_else(|| ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "invalid `[bundle]` section: extend path `{path}` requires an array in the manifest"
            ),
        })?;
    let mut combined = default_array.clone();
    combined.extend(current_array.iter().cloned());
    *current_array = combined;
    Ok(())
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

fn collect_value_paths(path: &str, value: &Value, out: &mut Vec<String>) {
    if !path.is_empty() {
        out.push(path.to_owned());
    }
    if let Some(table) = value.as_table() {
        for (key, child) in table {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            collect_value_paths(&child_path, child, out);
        }
    }
}
