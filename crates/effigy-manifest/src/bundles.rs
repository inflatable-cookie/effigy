use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;
use toml::Value;

use crate::ManifestError;

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
    }
    let (defaults, source_path) = match &selection {
        BundleSelection::Shipped { name } => (
            resolve_bundle_defaults(manifest_path, name, &normalized_inputs)?,
            bundle_source_path(name),
        ),
        BundleSelection::Local { path } => {
            resolve_local_bundle_defaults(manifest_path, path, &normalized_inputs)?
        }
    };
    let bundle_root = match &selection {
        BundleSelection::Shipped { name } => {
            materialize_shipped_bundle_assets(manifest_path, name)?
        }
        BundleSelection::Local { path } => path.clone(),
    };
    merge_missing_values(current, &defaults);
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
    resolve_bundle_defaults(&bundle_source_path(name), name, inputs)
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
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    match bundle_name {
        "decodelabs" => resolve_decodelabs_bundle(manifest_path, inputs),
        "decodelabs-library" => resolve_decodelabs_library_bundle(manifest_path, inputs),
        "underlay" => resolve_underlay_bundle(manifest_path, inputs),
        other => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("unknown bundle `{other}`"),
        }),
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalBundleDescriptor {
    bundle: LocalBundleMetadata,
    #[serde(default)]
    inputs: Vec<LocalBundleInputDescriptor>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalBundleMetadata {
    name: String,
    #[serde(default, rename = "description")]
    _description: String,
    #[serde(default = "default_local_bundle_defaults_file")]
    defaults: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalBundleInputDescriptor {
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
    let descriptor =
        toml::from_str::<LocalBundleDescriptor>(&descriptor_source).map_err(|error| {
            ManifestError::Parse {
                path: descriptor_path.clone(),
                error,
            }
        })?;
    validate_local_bundle_descriptor(manifest_path, &descriptor)?;
    let resolved_inputs = resolve_local_bundle_inputs(manifest_path, &descriptor, inputs)?;

    let defaults_path = bundle_dir.join(&descriptor.bundle.defaults);
    let defaults_template =
        std::fs::read_to_string(&defaults_path).map_err(|error| ManifestError::Read {
            path: defaults_path.clone(),
            error,
        })?;
    let rendered = render_local_bundle_template(
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
    if bundle_name != "decodelabs-library" {
        return Ok(());
    }
    let shared_root_path = bundle_shared_root_path(manifest_path, bundle_name, inputs)?;
    inputs.insert(
        "shared_root".to_owned(),
        Value::String(shared_root_path.display().to_string()),
    );
    if !inputs.contains_key("workspace_subdir") {
        let workspace_subdir =
            derive_bundle_workspace_subdir(manifest_path, &shared_root_path.display().to_string())?;
        inputs.insert(
            "workspace_subdir".to_owned(),
            Value::String(workspace_subdir),
        );
    }
    Ok(())
}

fn normalize_database_bundle_inputs(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &mut BTreeMap<String, Value>,
) -> Result<(), ManifestError> {
    let Some(databases) =
        normalize_database_value(manifest_path, bundle_name, "databases", inputs)?
    else {
        return Ok(());
    };

    if !inputs.contains_key("databases") {
        inputs.insert("databases".to_owned(), Value::Array(databases.clone()));
    }
    if !inputs.contains_key("database") {
        let Some(primary) = databases.first().and_then(|value| value.as_str()) else {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("bundle `{bundle_name}` normalized `databases` but found no primary database entry"),
            });
        };
        inputs.insert("database".to_owned(), Value::String(primary.to_owned()));
    }
    Ok(())
}

fn normalize_database_value(
    manifest_path: &Path,
    bundle_name: &str,
    field_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Option<Vec<Value>>, ManifestError> {
    match (inputs.get("databases"), inputs.get("database")) {
        (Some(Value::Array(values)), _) => {
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
        (Some(_), _) => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "bundle `{bundle_name}` input `{field_name}` must be a list of non-empty strings"
            ),
        }),
        (None, Some(Value::String(value))) => {
            let value = value.trim();
            if value.is_empty() {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("bundle `{bundle_name}` input `database` must not be empty"),
                });
            }
            Ok(Some(vec![Value::String(value.to_owned())]))
        }
        (None, Some(_)) => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` input `database` must be a non-empty string"),
        }),
        (None, None) => Ok(None),
    }
}

fn render_local_bundle_template(
    manifest_path: &Path,
    bundle_name: &str,
    bundle_dir: &Path,
    template: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<String, ManifestError> {
    let mut env = minijinja::Environment::new();
    env.add_template("bundle", template)
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("local bundle `{bundle_name}` template parse error: {error}"),
        })?;
    let template = env
        .get_template("bundle")
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("local bundle `{bundle_name}` template load error: {error}"),
        })?;
    let bundle_root = bundle_dir.display().to_string();
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
            detail: format!("local bundle `{bundle_name}` template render error: {error}"),
        })
}

#[derive(serde::Serialize)]
struct LocalBundleTemplateContext<'a> {
    inputs: &'a BTreeMap<String, Value>,
    bundle: LocalBundleTemplateBundle<'a>,
}

#[derive(serde::Serialize)]
struct LocalBundleTemplateBundle<'a> {
    name: &'a str,
    root: &'a str,
}

fn decodelabs_spec() -> BundleSpec {
    BundleSpec {
        name: "decodelabs".to_owned(),
        description: "DecodeLabs legacy PHP stack with nginx, php-fpm, MariaDB, phpMyAdmin, Redis, Memcached, a dev workspace, and local gateway routes.".to_owned(),
        inputs: vec![
            BundleInputSpec {
                name: "host".to_owned(),
                value_type: BundleInputType::String,
                required: true,
                description: "Primary local hostname for the app route; phpMyAdmin is published at `pma.<host>`.".to_owned(),
                default: None,
                example: Some(Value::String("contact-patch.legacy.test".to_owned())),
            },
            BundleInputSpec {
                name: "project_name".to_owned(),
                value_type: BundleInputType::String,
                required: true,
                description: "Compose project name used for the generated container environment.".to_owned(),
                default: None,
                example: Some(Value::String("contactpatch-dev".to_owned())),
            },
            BundleInputSpec {
                name: "database".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Primary MariaDB database name for the app and bundled db alias rendering. Kept for backwards compatibility; prefer `databases`.".to_owned(),
                default: None,
                example: Some(Value::String("contactpatch".to_owned())),
            },
            BundleInputSpec {
                name: "databases".to_owned(),
                value_type: BundleInputType::List,
                required: false,
                description: "MariaDB databases to create for the stack. The first entry becomes the primary app database.".to_owned(),
                default: None,
                example: Some(Value::Array(vec![
                    Value::String("contactpatch".to_owned()),
                    Value::String("contactpatch_test".to_owned()),
                ])),
            },
            BundleInputSpec {
                name: "system_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[systems.<name>]` block rendered by the bundle.".to_owned(),
                default: Some(Value::String("dev".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "container_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[containers.<name>]` block that hosts the stack (also used as the default container).".to_owned(),
                default: Some(Value::String("web".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "workspace_service_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the php-fpm workspace service inside the container (also wired as the `php` alias target and the `composer` service).".to_owned(),
                default: Some(Value::String("app".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "default_workspace".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[systems.<system>.workspaces.<name>]` workspace treated as the system default.".to_owned(),
                default: Some(Value::String("app".to_owned())),
                example: None,
            },
        ],
    }
}

fn decodelabs_library_spec() -> BundleSpec {
    BundleSpec {
        name: "decodelabs-library".to_owned(),
        description: "DecodeLabs library workspace with a shared php-fpm dev container, explicit container-side Composer-global Effigy deferral, and no default web/db/gateway services.".to_owned(),
        inputs: vec![
            BundleInputSpec {
                name: "shared_root".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Host path mounted into the shared library container root. Defaults to the parent directory of the consuming repo (`../`).".to_owned(),
                default: Some(Value::String("../".to_owned())),
                example: Some(Value::String("../".to_owned())),
            },
            BundleInputSpec {
                name: "workspace_subdir".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Repo path under `shared_root` used as the workspace CWD. Defaults to the consuming repo's relative path under `shared_root`.".to_owned(),
                default: None,
                example: Some(Value::String("collections".to_owned())),
            },
            BundleInputSpec {
                name: "project_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Compose project name for the shared library container runtime.".to_owned(),
                default: Some(Value::String("decodelabs-library-dev".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "system_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[systems.<name>]` block rendered by the bundle.".to_owned(),
                default: Some(Value::String("dev".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "container_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[containers.<name>]` block hosting the shared library runtime.".to_owned(),
                default: Some(Value::String("web".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "workspace_service_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the php-fpm workspace service inside the shared runtime container.".to_owned(),
                default: Some(Value::String("app".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "default_workspace".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[systems.<system>.workspaces.<name>]` workspace treated as the system default.".to_owned(),
                default: Some(Value::String("app".to_owned())),
                example: None,
            },
        ],
    }
}

const DECODELABS_PHP_EXTENSIONS: &[&str] = &[
    "pdo_mysql",
    "intl",
    "exif",
    "zip",
    "gd",
    "redis",
    "memcached",
    "opcache",
];

fn resolve_decodelabs_bundle(
    manifest_path: &Path,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    let host = required_bundle_string(manifest_path, "decodelabs", inputs, "host")?;
    let project_name = required_bundle_string(manifest_path, "decodelabs", inputs, "project_name")?;
    let database = required_bundle_string(manifest_path, "decodelabs", inputs, "database")?;
    let system_name =
        optional_bundle_string(inputs, "system_name").unwrap_or_else(|| "dev".to_owned());
    let container_name =
        optional_bundle_string(inputs, "container_name").unwrap_or_else(|| "web".to_owned());
    let workspace_service_name = optional_bundle_string(inputs, "workspace_service_name")
        .unwrap_or_else(|| "app".to_owned());
    let default_workspace =
        optional_bundle_string(inputs, "default_workspace").unwrap_or_else(|| "app".to_owned());

    let template = r#"
[containers]
default = "__CONTAINER_NAME__"

[containers.__CONTAINER_NAME__]
driver = "colima"
startup = "detached"
project_name = "__PROJECT_NAME__"
primary_service = "__WORKSPACE_SERVICE_NAME__"
working_dir = "/var/www/html"

[containers.__CONTAINER_NAME__.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.__CONTAINER_NAME__.dns]
routes = [
  { domain = "__HOST__", tls = true, service = "web" },
  { domain = "pma.__HOST__", tls = true, service = "pma" },
]

[containers.__CONTAINER_NAME__.aliases]
php = "__WORKSPACE_SERVICE_NAME__"
composer = { service = "__WORKSPACE_SERVICE_NAME__", command = "composer" }
mysql = { service = "db", command = "mysql -uroot{% if services.db.params.root_password %} -p{{ services.db.params.root_password }}{% endif %} {{ services.db.params.database }}" }

[containers.__CONTAINER_NAME__.services.__WORKSPACE_SERVICE_NAME__]
catalog = "php-fpm"
version = "8.4"
document_root = "."
node_version = "20"
node_global_packages = ["eclint"]
composer_global_packages = ["decodelabs/effigy"]
extensions = [
__PHP_EXTENSIONS__
]

[containers.__CONTAINER_NAME__.services.web]
catalog = "nginx"
variant = "decodelabs"
document_root = "."

[containers.__CONTAINER_NAME__.services.db]
catalog = "mariadb"
version = "10.11"
database = "__DATABASE__"
databases = __DATABASES__

[containers.__CONTAINER_NAME__.services.pma]
catalog = "phpmyadmin"
version = "latest"
database_host = "db"

[containers.__CONTAINER_NAME__.services.memcache]
catalog = "memcached"
memory = 128

[containers.__CONTAINER_NAME__.services.redis]
catalog = "redis"
version = "7"

[systems]
default = "__SYSTEM_NAME__"

[systems.__SYSTEM_NAME__]
default_workspace = "__DEFAULT_WORKSPACE__"

[systems.__SYSTEM_NAME__.workspaces.__DEFAULT_WORKSPACE__]
container = "__CONTAINER_NAME__"

[tasks.dev]
workspace = "__DEFAULT_WORKSPACE__"

[tasks.seed]
workspace = "__DEFAULT_WORKSPACE__"
run = [{ rhai = "{{ bundle.root }}/scripts/seed-latest-db-dump.rhai" }]

[tasks.release]
run = "composer global exec effigy -- release"

[defer]
run = "composer global exec effigy -- {request} {args}"
run_in = "container"
"#;

    let rendered = template
        .replace("__HOST__", &host)
        .replace("__PROJECT_NAME__", &project_name)
        .replace("__DATABASE__", &database)
        .replace(
            "__DATABASES__",
            &render_toml_string_list(inputs, "databases"),
        )
        .replace("__SYSTEM_NAME__", &system_name)
        .replace("__CONTAINER_NAME__", &container_name)
        .replace("__WORKSPACE_SERVICE_NAME__", &workspace_service_name)
        .replace(
            "__PHP_EXTENSIONS__",
            &render_toml_string_array_lines(DECODELABS_PHP_EXTENSIONS, "  "),
        )
        .replace("__DEFAULT_WORKSPACE__", &default_workspace);

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("decodelabs"),
        error,
    })
}

fn resolve_decodelabs_library_bundle(
    manifest_path: &Path,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    let shared_root_mount = bundle_shared_root_path(manifest_path, "decodelabs-library", inputs)?;
    let workspace_subdir = optional_bundle_string(inputs, "workspace_subdir")
        .map(Ok)
        .unwrap_or_else(|| {
            derive_bundle_workspace_subdir(manifest_path, &shared_root_mount.display().to_string())
        })?;
    let project_name = optional_bundle_string(inputs, "project_name")
        .unwrap_or_else(|| "decodelabs-library-dev".to_owned());
    let system_name =
        optional_bundle_string(inputs, "system_name").unwrap_or_else(|| "dev".to_owned());
    let container_name =
        optional_bundle_string(inputs, "container_name").unwrap_or_else(|| "web".to_owned());
    let workspace_service_name = optional_bundle_string(inputs, "workspace_service_name")
        .unwrap_or_else(|| "app".to_owned());
    let default_workspace =
        optional_bundle_string(inputs, "default_workspace").unwrap_or_else(|| "app".to_owned());

    let template = r#"
[containers]
default = "__CONTAINER_NAME__"

[containers.__CONTAINER_NAME__]
driver = "colima"
startup = "detached"
project_name = "__PROJECT_NAME__"
primary_service = "__WORKSPACE_SERVICE_NAME__"
working_dir = "/workspace-root"

[containers.__CONTAINER_NAME__.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.__CONTAINER_NAME__.aliases]
php = "__WORKSPACE_SERVICE_NAME__"
composer = { service = "__WORKSPACE_SERVICE_NAME__", command = "composer" }

[containers.__CONTAINER_NAME__.services.__WORKSPACE_SERVICE_NAME__]
catalog = "php-fpm"
version = "8.4"
document_root = "."
working_dir = "/workspace-root"
mount_source = "__SHARED_ROOT__"
node_version = "20"
node_global_packages = ["eclint"]
composer_global_packages = ["decodelabs/effigy"]
extensions = [
__PHP_EXTENSIONS__
]

[systems]
default = "__SYSTEM_NAME__"

[systems.__SYSTEM_NAME__]
container = "__CONTAINER_NAME__"
default_workspace = "__DEFAULT_WORKSPACE__"
working_dir = "/workspace-root/__WORKSPACE_SUBDIR__"
user = "dev"
home = "/home/dev"

[systems.__SYSTEM_NAME__.workspaces.__DEFAULT_WORKSPACE__]

[tasks.dev]
workspace = "__DEFAULT_WORKSPACE__"

[defer]
run = "composer global exec effigy -- {request} {args}"
run_in = "container"
"#;

    let rendered = template
        .replace("__CONTAINER_NAME__", &container_name)
        .replace("__PROJECT_NAME__", &project_name)
        .replace("__SHARED_ROOT__", &shared_root_mount.display().to_string())
        .replace("__SYSTEM_NAME__", &system_name)
        .replace("__DEFAULT_WORKSPACE__", &default_workspace)
        .replace("__WORKSPACE_SERVICE_NAME__", &workspace_service_name)
        .replace("__WORKSPACE_SUBDIR__", &workspace_subdir)
        .replace(
            "__PHP_EXTENSIONS__",
            &render_toml_string_array_lines(DECODELABS_PHP_EXTENSIONS, "  "),
        );

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("decodelabs-library"),
        error,
    })
}

fn underlay_spec() -> BundleSpec {
    BundleSpec {
        name: "underlay".to_owned(),
        description: "Underlay-style Rust + Bun workspace stack with one long-running workspace container, bundled postgres/dbgate/mailpit/minio services, and gateway-published app routes plus loopback alias discovery for db/smtp/s3.".to_owned(),
        inputs: vec![
            BundleInputSpec {
                name: "host".to_owned(),
                value_type: BundleInputType::String,
                required: true,
                description: "Primary local hostname for the front-end route; bundle defaults also publish `admin.<host>`, `api.<host>`, `dbgate.<host>`, `mailpit.<host>`, and `minio.<host>`.".to_owned(),
                default: None,
                example: Some(Value::String("acme.test".to_owned())),
            },
            BundleInputSpec {
                name: "project_name".to_owned(),
                value_type: BundleInputType::String,
                required: true,
                description: "Compose project name used for the generated dev stack.".to_owned(),
                default: None,
                example: Some(Value::String("underlay-reference-dev".to_owned())),
            },
            BundleInputSpec {
                name: "workspace_subdir".to_owned(),
                value_type: BundleInputType::String,
                required: true,
                description: "Repo path under `/workspace-root` for the workspace container's working directory.".to_owned(),
                default: None,
                example: Some(Value::String("underlay-reference".to_owned())),
            },
            BundleInputSpec {
                name: "database".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Primary Postgres database name for the bundled postgres service. Kept for backwards compatibility; prefer `databases`.".to_owned(),
                default: None,
                example: Some(Value::String("acme".to_owned())),
            },
            BundleInputSpec {
                name: "databases".to_owned(),
                value_type: BundleInputType::List,
                required: false,
                description: "Postgres databases to create for the stack. The first entry becomes the primary app database.".to_owned(),
                default: None,
                example: Some(Value::Array(vec![
                    Value::String("acme".to_owned()),
                    Value::String("acme_test".to_owned()),
                ])),
            },
            BundleInputSpec {
                name: "api_port".to_owned(),
                value_type: BundleInputType::Integer,
                required: false,
                description: "Host and workspace port for the API dev server.".to_owned(),
                default: Some(Value::Integer(41001)),
                example: None,
            },
            BundleInputSpec {
                name: "admin_port".to_owned(),
                value_type: BundleInputType::Integer,
                required: false,
                description: "Host and workspace port for the admin dev server.".to_owned(),
                default: Some(Value::Integer(41002)),
                example: None,
            },
            BundleInputSpec {
                name: "front_port".to_owned(),
                value_type: BundleInputType::Integer,
                required: false,
                description: "Host and workspace port for the public front-end dev server.".to_owned(),
                default: Some(Value::Integer(41003)),
                example: None,
            },
            BundleInputSpec {
                name: "dirs.front".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Front-end package directory for the bundled UI setup helper. When omitted, the helper falls back to the shipped Underlay guesses.".to_owned(),
                default: None,
                example: Some(Value::String("cream".to_owned())),
            },
            BundleInputSpec {
                name: "dirs.admin".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Admin package directory for the bundled UI setup helper. When omitted, the helper falls back to the shipped Underlay guesses.".to_owned(),
                default: None,
                example: Some(Value::String("dairy".to_owned())),
            },
            BundleInputSpec {
                name: "dirs.ui".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Shared UI package directory the bundled UI setup helper should hydrate before front/admin startup. When omitted, the helper falls back to the shipped Underlay guesses.".to_owned(),
                default: None,
                example: Some(Value::String("froyo".to_owned())),
            },
            BundleInputSpec {
                name: "routes.front".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Gateway subdomain label for the front-end route. Empty keeps the bare host.".to_owned(),
                default: Some(Value::String(String::new())),
                example: Some(Value::String("cream".to_owned())),
            },
            BundleInputSpec {
                name: "routes.admin".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Gateway subdomain label for the admin route.".to_owned(),
                default: Some(Value::String("admin".to_owned())),
                example: Some(Value::String("dairy".to_owned())),
            },
            BundleInputSpec {
                name: "routes.api".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Gateway subdomain label for the API route.".to_owned(),
                default: Some(Value::String("api".to_owned())),
                example: Some(Value::String("farmyard".to_owned())),
            },
            BundleInputSpec {
                name: "system_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[systems.<name>]` block rendered by the bundle.".to_owned(),
                default: Some(Value::String("dev".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "container_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[containers.<name>]` block that hosts the stack (also used as the default container).".to_owned(),
                default: Some(Value::String("stack".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "workspace_service_name".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the long-running workspace service inside the container (referenced by `primary_service` and by the published HTTP routes).".to_owned(),
                default: Some(Value::String("workspace".to_owned())),
                example: None,
            },
            BundleInputSpec {
                name: "default_workspace".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Name of the `[systems.<system>.workspaces.<name>]` workspace treated as the system default.".to_owned(),
                default: Some(Value::String("app".to_owned())),
                example: None,
            },
        ],
    }
}

fn resolve_underlay_bundle(
    manifest_path: &Path,
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
    let front_route_domain = underlay_route_domain(
        &host,
        optional_bundle_string(inputs, "routes.front").as_deref(),
    );
    let admin_route_domain = underlay_route_domain(
        &host,
        optional_bundle_string(inputs, "routes.admin")
            .as_deref()
            .or(Some("admin")),
    );
    let api_route_domain = underlay_route_domain(
        &host,
        optional_bundle_string(inputs, "routes.api")
            .as_deref()
            .or(Some("api")),
    );
    let system_name =
        optional_bundle_string(inputs, "system_name").unwrap_or_else(|| "dev".to_owned());
    let container_name =
        optional_bundle_string(inputs, "container_name").unwrap_or_else(|| "stack".to_owned());
    let workspace_service_name = optional_bundle_string(inputs, "workspace_service_name")
        .unwrap_or_else(|| "workspace".to_owned());
    let default_workspace =
        optional_bundle_string(inputs, "default_workspace").unwrap_or_else(|| "app".to_owned());
    let bundle_root = materialize_shipped_bundle_assets(manifest_path, "underlay")?;

    let template = r#"
[package_manager]
js = "bun"

[systems]
default = "__SYSTEM_NAME__"

[systems.__SYSTEM_NAME__]
container = "__CONTAINER_NAME__"
default_workspace = "__DEFAULT_WORKSPACE__"
working_dir = "/workspace-root/__WORKSPACE_SUBDIR__"
user = "dev"
home = "/home/dev"
mounts = []

[systems.__SYSTEM_NAME__.workspaces.__DEFAULT_WORKSPACE__]

[containers]
default = "__CONTAINER_NAME__"

[containers.__CONTAINER_NAME__]
startup = "detached"
context = "__SYSTEM_NAME__"
project_name = "__PROJECT_NAME__"
primary_service = "__WORKSPACE_SERVICE_NAME__"

[containers.__CONTAINER_NAME__.aliases]
psql = "postgres"

[containers.__CONTAINER_NAME__.services.__WORKSPACE_SERVICE_NAME__]
catalog = "workspace-rust-bun"
working_subdir = "__WORKSPACE_SUBDIR__"
host_ports = [
  "__API_PORT__:__API_PORT__",
  "__ADMIN_PORT__:__ADMIN_PORT__",
  "__FRONT_PORT__:__FRONT_PORT__",
]

[containers.__CONTAINER_NAME__.services.postgres]
catalog = "postgres"
database = "__DATABASE__"
databases = __DATABASES__
password = "postgres"

[containers.__CONTAINER_NAME__.services.dbgate]
catalog = "dbgate"
database_host = "postgres"
database = "__DATABASE__"
connection_label = "__PROJECT_NAME__"

[containers.__CONTAINER_NAME__.services.mailpit]
catalog = "mailpit"

[containers.__CONTAINER_NAME__.services.minio]
catalog = "minio"

[containers.__CONTAINER_NAME__.dns]
routes = [
  { domain = "__FRONT_ROUTE_DOMAIN__", tls = true, port = __FRONT_PORT__, service = "__WORKSPACE_SERVICE_NAME__" },
  { domain = "__ADMIN_ROUTE_DOMAIN__", tls = true, port = __ADMIN_PORT__, service = "__WORKSPACE_SERVICE_NAME__" },
  { domain = "__API_ROUTE_DOMAIN__", tls = true, port = __API_PORT__, service = "__WORKSPACE_SERVICE_NAME__" },
  { domain = "dbgate.__HOST__", tls = true, port = 3000, service = "dbgate" },
  { domain = "mailpit.__HOST__", port = 8025, service = "mailpit" },
  { domain = "minio.__HOST__", port = 9001, service = "minio" },
]

[tasks."smoke:error-logging"]
run = [{ rhai = "{{ bundle.root }}/scripts/error-reporting.rhai" }]
run_in = "host"

[tasks."metrics:error-log"]
run = [{ rhai = "{{ bundle.root }}/scripts/error-reporting.rhai" }]
run_in = "host"

[tasks."validate:error-reporting"]
run = [{ rhai = "{{ bundle.root }}/scripts/error-reporting.rhai" }]
run_in = "host"
"#;

    let rendered = render_shipped_bundle_template(
        manifest_path,
        "underlay",
        &bundle_root,
        &template
            .replace("__HOST__", &host)
            .replace("__PROJECT_NAME__", &project_name)
            .replace("__WORKSPACE_SUBDIR__", &workspace_subdir)
            .replace("__DATABASE__", &database)
            .replace("__FRONT_ROUTE_DOMAIN__", &front_route_domain)
            .replace("__ADMIN_ROUTE_DOMAIN__", &admin_route_domain)
            .replace("__API_ROUTE_DOMAIN__", &api_route_domain)
            .replace(
                "__DATABASES__",
                &render_toml_string_list(inputs, "databases"),
            )
            .replace("__API_PORT__", &api_port.to_string())
            .replace("__ADMIN_PORT__", &admin_port.to_string())
            .replace("__FRONT_PORT__", &front_port.to_string())
            .replace("__SYSTEM_NAME__", &system_name)
            .replace("__CONTAINER_NAME__", &container_name)
            .replace("__WORKSPACE_SERVICE_NAME__", &workspace_service_name)
            .replace("__DEFAULT_WORKSPACE__", &default_workspace),
    )?;

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("underlay"),
        error,
    })
}

fn render_shipped_bundle_template(
    manifest_path: &Path,
    bundle_name: &str,
    bundle_root: &Path,
    template: &str,
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
    template
        .render(ShippedBundleTemplateContext {
            bundle: LocalBundleTemplateBundle {
                name: bundle_name,
                root: &bundle_root,
            },
        })
        .map_err(|error| ManifestError::Render {
            path: manifest_path.to_path_buf(),
            detail: format!("bundle `{bundle_name}` template render error: {error}"),
        })
}

#[derive(serde::Serialize)]
struct ShippedBundleTemplateContext<'a> {
    bundle: LocalBundleTemplateBundle<'a>,
}

#[derive(Clone, Copy)]
struct EmbeddedBundleAsset {
    path: &'static str,
    contents: &'static str,
}

struct BundleExportFile {
    path: &'static str,
    contents: String,
}

const DECODELABS_ASSETS: &[EmbeddedBundleAsset] = &[EmbeddedBundleAsset {
    path: "scripts/seed-latest-db-dump.rhai",
    contents: include_str!(
        "../../effigy-catalog/starters/decodelabs/scripts/seed-latest-db-dump.rhai"
    ),
}];

const UNDERLAY_ASSETS: &[EmbeddedBundleAsset] = &[
    EmbeddedBundleAsset {
        path: "scripts/bootstrap-env.rhai",
        contents: include_str!("../../effigy-catalog/starters/underlay/scripts/bootstrap-env.rhai"),
    },
    EmbeddedBundleAsset {
        path: "scripts/dev/ui-setup.rhai",
        contents: include_str!("../../effigy-catalog/starters/underlay/scripts/dev/ui-setup.rhai"),
    },
    EmbeddedBundleAsset {
        path: "scripts/error-reporting.rhai",
        contents: include_str!(
            "../../effigy-catalog/starters/underlay/scripts/error-reporting.rhai"
        ),
    },
];

fn shipped_bundle_export_files(name: &str) -> Result<Vec<BundleExportFile>, ManifestError> {
    let spec = get_bundle(name).ok_or_else(|| ManifestError::Compose {
        path: bundle_source_path(name),
        detail: format!("unknown bundle `{name}`"),
    })?;
    let defaults = match name {
        "decodelabs" => decodelabs_export_template(),
        "decodelabs-library" => decodelabs_library_export_template(),
        "underlay" => underlay_export_template(),
        other => {
            return Err(ManifestError::Compose {
                path: bundle_source_path(other),
                detail: format!("unknown bundle `{other}`"),
            });
        }
    };

    let mut files = vec![
        BundleExportFile {
            path: "bundle.toml",
            contents: render_export_descriptor(&spec),
        },
        BundleExportFile {
            path: "effigy.toml",
            contents: defaults,
        },
        BundleExportFile {
            path: "README.md",
            contents: render_export_readme(&spec),
        },
    ];
    for asset in embedded_bundle_assets(name) {
        files.push(BundleExportFile {
            path: asset.path,
            contents: asset.contents.to_owned(),
        });
    }
    Ok(files)
}

fn render_export_descriptor(spec: &BundleSpec) -> String {
    let mut out = String::new();
    out.push_str("[bundle]\n");
    out.push_str(&format!("name = {}\n", toml_string(&spec.name)));
    out.push_str(&format!(
        "description = {}\n",
        toml_string(&spec.description)
    ));
    out.push_str("defaults = \"effigy.toml\"\n");
    for input in &spec.inputs {
        out.push_str("\n[[inputs]]\n");
        out.push_str(&format!("name = {}\n", toml_string(&input.name)));
        out.push_str(&format!(
            "type = \"{}\"\n",
            bundle_input_type_literal(input.value_type)
        ));
        if input.required {
            out.push_str("required = true\n");
        }
        out.push_str(&format!(
            "description = {}\n",
            toml_string(&input.description)
        ));
        if let Some(default) = &input.default {
            out.push_str(&format!("default = {default}\n"));
        }
        if let Some(example) = &input.example {
            out.push_str(&format!("example = {example}\n"));
        }
    }
    out
}

fn render_export_readme(spec: &BundleSpec) -> String {
    format!(
        "# {} bundle\n\n{}\n\nUse from a consuming manifest with:\n\n```toml\n[bundle]\nbase_path = \"path/to/{}\"\n# set the inputs from bundle.toml here\n```\n",
        spec.name, spec.description, spec.name
    )
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

fn bundle_input_type_literal(value_type: BundleInputType) -> &'static str {
    match value_type {
        BundleInputType::String => "string",
        BundleInputType::Integer => "integer",
        BundleInputType::Bool => "bool",
        BundleInputType::List => "list",
    }
}

fn decodelabs_export_template() -> String {
    let extensions = render_toml_string_array_lines(DECODELABS_PHP_EXTENSIONS, "  ");
    format!(
        r#"[containers]
default = "{{{{ inputs.container_name }}}}"

[containers.{{{{ inputs.container_name }}}}]
driver = "colima"
startup = "detached"
project_name = "{{{{ inputs.project_name }}}}"
primary_service = "{{{{ inputs.workspace_service_name }}}}"
working_dir = "/var/www/html"

[containers.{{{{ inputs.container_name }}}}.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.{{{{ inputs.container_name }}}}.dns]
routes = [
  {{ domain = "{{{{ inputs.host }}}}", tls = true, service = "web" }},
  {{ domain = "pma.{{{{ inputs.host }}}}", tls = true, service = "pma" }},
]

[containers.{{{{ inputs.container_name }}}}.aliases]
php = "{{{{ inputs.workspace_service_name }}}}"
composer = {{ service = "{{{{ inputs.workspace_service_name }}}}", command = "composer" }}
mysql = {{ service = "db", command = "mysql -uroot{{% raw %}}{{% if services.db.params.root_password %}} -p{{{{ services.db.params.root_password }}}}{{% endif %}}{{% endraw %}} {{{{ inputs.database }}}}" }}

[containers.{{{{ inputs.container_name }}}}.services.{{{{ inputs.workspace_service_name }}}}]
catalog = "php-fpm"
version = "8.4"
document_root = "."
node_version = "20"
node_global_packages = ["eclint"]
composer_global_packages = ["decodelabs/effigy"]
extensions = [
{extensions}
]

[containers.{{{{ inputs.container_name }}}}.services.web]
catalog = "nginx"
variant = "decodelabs"
document_root = "."
service = "{{{{ inputs.workspace_service_name }}}}"

[containers.{{{{ inputs.container_name }}}}.services.db]
catalog = "mariadb"
database = "{{{{ inputs.database }}}}"
databases = [{{% for database in inputs.databases %}}"{{{{ database }}}}"{{% if not loop.last %}}, {{% endif %}}{{% endfor %}}]

[containers.{{{{ inputs.container_name }}}}.services.pma]
catalog = "phpmyadmin"
database_host = "db"

[containers.{{{{ inputs.container_name }}}}.services.memcache]
catalog = "memcached"
memory = 128

[containers.{{{{ inputs.container_name }}}}.services.redis]
catalog = "redis"

[systems]
default = "{{{{ inputs.system_name }}}}"

[systems.{{{{ inputs.system_name }}}}]
default_workspace = "{{{{ inputs.default_workspace }}}}"
container = "{{{{ inputs.container_name }}}}"

[systems.{{{{ inputs.system_name }}}}.workspaces.{{{{ inputs.default_workspace }}}}]

[tasks.dev]
workspace = "{{{{ inputs.default_workspace }}}}"

[tasks.seed]
workspace = "{{{{ inputs.default_workspace }}}}"
run = [{{ rhai = "{{{{ bundle.root }}}}/scripts/seed-latest-db-dump.rhai" }}]

[tasks.release]
run = "composer global exec effigy -- release"

[defer]
run = "composer global exec effigy -- {{request}} {{args}}"
run_in = "container"
"#
    )
}

fn decodelabs_library_export_template() -> String {
    let extensions = render_toml_string_array_lines(DECODELABS_PHP_EXTENSIONS, "  ");
    format!(
        r#"[containers]
default = "{{{{ inputs.container_name }}}}"

[containers.{{{{ inputs.container_name }}}}]
driver = "colima"
startup = "detached"
project_name = "{{{{ inputs.project_name }}}}"
primary_service = "{{{{ inputs.workspace_service_name }}}}"
working_dir = "/workspace-root"

[containers.{{{{ inputs.container_name }}}}.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.{{{{ inputs.container_name }}}}.aliases]
php = "{{{{ inputs.workspace_service_name }}}}"
composer = {{ service = "{{{{ inputs.workspace_service_name }}}}", command = "composer" }}

[containers.{{{{ inputs.container_name }}}}.services.{{{{ inputs.workspace_service_name }}}}]
catalog = "php-fpm"
version = "8.4"
document_root = "."
working_dir = "/workspace-root"
mount_source = "{{{{ inputs.shared_root }}}}"
node_version = "20"
node_global_packages = ["eclint"]
composer_global_packages = ["decodelabs/effigy"]
extensions = [
{extensions}
]

[systems]
default = "{{{{ inputs.system_name }}}}"

[systems.{{{{ inputs.system_name }}}}]
container = "{{{{ inputs.container_name }}}}"
default_workspace = "{{{{ inputs.default_workspace }}}}"
working_dir = "/workspace-root/{{{{ inputs.workspace_subdir }}}}"
user = "dev"
home = "/home/dev"

[systems.{{{{ inputs.system_name }}}}.workspaces.{{{{ inputs.default_workspace }}}}]

[tasks.dev]
workspace = "{{{{ inputs.default_workspace }}}}"

[defer]
run = "composer global exec effigy -- {{request}} {{args}}"
run_in = "container"
"#
    )
}

const UNDERLAY_EXPORT_TEMPLATE: &str = r#"[package_manager]
js = "bun"

[systems]
default = "{{ inputs.system_name }}"

[systems.{{ inputs.system_name }}]
container = "{{ inputs.container_name }}"
default_workspace = "{{ inputs.default_workspace }}"
working_dir = "/workspace-root/{{ inputs.workspace_subdir }}"
user = "dev"
home = "/home/dev"
mounts = []

[systems.{{ inputs.system_name }}.workspaces.{{ inputs.default_workspace }}]

[containers]
default = "{{ inputs.container_name }}"

[containers.{{ inputs.container_name }}]
startup = "detached"
context = "{{ inputs.system_name }}"
project_name = "{{ inputs.project_name }}"
primary_service = "{{ inputs.workspace_service_name }}"

[containers.{{ inputs.container_name }}.aliases]
psql = "postgres"

[containers.{{ inputs.container_name }}.services.{{ inputs.workspace_service_name }}]
catalog = "workspace-rust-bun"
working_subdir = "{{ inputs.workspace_subdir }}"
host_ports = [
  "{{ inputs.api_port }}:{{ inputs.api_port }}",
  "{{ inputs.admin_port }}:{{ inputs.admin_port }}",
  "{{ inputs.front_port }}:{{ inputs.front_port }}",
]

[containers.{{ inputs.container_name }}.services.postgres]
catalog = "postgres"
database = "{{ inputs.database }}"
databases = [{% for database in inputs.databases %}"{{ database }}"{% if not loop.last %}, {% endif %}{% endfor %}]
password = "postgres"

[containers.{{ inputs.container_name }}.services.dbgate]
catalog = "dbgate"
database_host = "postgres"
database = "{{ inputs.database }}"
connection_label = "{{ inputs.project_name }}"

[containers.{{ inputs.container_name }}.services.mailpit]
catalog = "mailpit"

[containers.{{ inputs.container_name }}.services.minio]
catalog = "minio"

[containers.{{ inputs.container_name }}.dns]
routes = [
  { domain = "{% if inputs.routes.front %}{{ inputs.routes.front }}.{{ inputs.host }}{% else %}{{ inputs.host }}{% endif %}", tls = true, port = {{ inputs.front_port }}, service = "{{ inputs.workspace_service_name }}" },
  { domain = "{% if inputs.routes.admin %}{{ inputs.routes.admin }}.{{ inputs.host }}{% else %}{{ inputs.host }}{% endif %}", tls = true, port = {{ inputs.admin_port }}, service = "{{ inputs.workspace_service_name }}" },
  { domain = "{% if inputs.routes.api %}{{ inputs.routes.api }}.{{ inputs.host }}{% else %}{{ inputs.host }}{% endif %}", tls = true, port = {{ inputs.api_port }}, service = "{{ inputs.workspace_service_name }}" },
  { domain = "dbgate.{{ inputs.host }}", tls = true, port = 3000, service = "dbgate" },
  { domain = "mailpit.{{ inputs.host }}", port = 8025, service = "mailpit" },
  { domain = "minio.{{ inputs.host }}", port = 9001, service = "minio" },
]

[tasks."smoke:error-logging"]
run = [{ rhai = "{{ bundle.root }}/scripts/error-reporting.rhai" }]
run_in = "host"

[tasks."metrics:error-log"]
run = [{ rhai = "{{ bundle.root }}/scripts/error-reporting.rhai" }]
run_in = "host"

[tasks."validate:error-reporting"]
run = [{ rhai = "{{ bundle.root }}/scripts/error-reporting.rhai" }]
run_in = "host"
"#;

fn underlay_export_template() -> String {
    UNDERLAY_EXPORT_TEMPLATE.to_owned()
}

fn materialize_shipped_bundle_assets(
    manifest_path: &Path,
    bundle_name: &str,
) -> Result<PathBuf, ManifestError> {
    let assets = embedded_bundle_assets(bundle_name);
    if assets.is_empty() || is_virtual_bundle_manifest_path(manifest_path) {
        return Ok(bundle_source_path(bundle_name));
    }

    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    ensure_effigy_ignored_in_git_root(manifest_dir).map_err(|error| ManifestError::Read {
        path: manifest_dir.join(".gitignore"),
        error,
    })?;
    let hash = embedded_bundle_assets_hash(bundle_name, assets);
    let bundle_cache_dir = manifest_dir
        .join(".effigy")
        .join("runtime")
        .join("bundles")
        .join(bundle_name);
    let bundle_root = bundle_cache_dir.join(&hash);
    prune_stale_materialized_bundle_roots(&bundle_cache_dir, &hash)?;

    for asset in assets {
        let asset_path = bundle_root.join(asset.path);
        if let Some(parent) = asset_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ManifestError::Read {
                path: parent.to_path_buf(),
                error,
            })?;
        }
        let should_write = match std::fs::read_to_string(&asset_path) {
            Ok(existing) => existing != asset.contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(error) => {
                return Err(ManifestError::Read {
                    path: asset_path,
                    error,
                });
            }
        };
        if should_write {
            std::fs::write(&asset_path, asset.contents).map_err(|error| ManifestError::Read {
                path: asset_path,
                error,
            })?;
        }
    }

    Ok(bundle_root)
}

fn prune_stale_materialized_bundle_roots(
    bundle_cache_dir: &Path,
    active_hash: &str,
) -> Result<(), ManifestError> {
    let entries = match std::fs::read_dir(bundle_cache_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(ManifestError::Read {
                path: bundle_cache_dir.to_path_buf(),
                error,
            });
        }
    };

    for entry in entries {
        let entry = entry.map_err(|error| ManifestError::Read {
            path: bundle_cache_dir.to_path_buf(),
            error,
        })?;
        let path = entry.path();
        if entry.file_name().to_string_lossy() == active_hash {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| ManifestError::Read {
            path: path.clone(),
            error,
        })?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|error| ManifestError::Read {
                path: path.clone(),
                error,
            })?;
        }
    }

    Ok(())
}

fn embedded_bundle_assets(bundle_name: &str) -> &'static [EmbeddedBundleAsset] {
    match bundle_name {
        "decodelabs" => DECODELABS_ASSETS,
        "decodelabs-library" => &[],
        "underlay" => UNDERLAY_ASSETS,
        _ => &[],
    }
}

fn is_virtual_bundle_manifest_path(manifest_path: &Path) -> bool {
    manifest_path.to_string_lossy().starts_with("<bundle:")
}

fn embedded_bundle_assets_hash(bundle_name: &str, assets: &[EmbeddedBundleAsset]) -> String {
    let mut hash = Fnv64::new();
    hash.write(bundle_name.as_bytes());
    for asset in assets {
        hash.write(asset.path.as_bytes());
        hash.write(asset.contents.as_bytes());
    }
    format!("{:016x}", hash.finish())
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn required_bundle_string(
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

fn bundle_shared_root_input(
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

fn bundle_shared_root_path(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<PathBuf, ManifestError> {
    let shared_root = bundle_shared_root_input(manifest_path, bundle_name, inputs)?;
    let shared_root_path = resolve_bundle_host_path(manifest_path, &shared_root);
    Ok(shared_root_path.canonicalize().unwrap_or(shared_root_path))
}

fn bundle_default_input_string(bundle_name: &str, key: &str) -> Option<String> {
    list_bundles()
        .into_iter()
        .find(|spec| spec.name == bundle_name)
        .and_then(|spec| spec.inputs.into_iter().find(|input| input.name == key))
        .and_then(|input| input.default)
        .and_then(|value| value.as_str().map(str::to_owned))
}

fn optional_bundle_integer(inputs: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    bundle_input_value(inputs, key).and_then(Value::as_integer)
}

fn optional_bundle_string(inputs: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    let value = bundle_input_value(inputs, key)?.as_str()?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn render_toml_string_list(inputs: &BTreeMap<String, Value>, key: &str) -> String {
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

fn render_toml_string_array_lines(values: &[&str], indent: &str) -> String {
    values
        .iter()
        .map(|value| format!("{indent}{value:?},"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn derive_bundle_workspace_subdir(
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

fn resolve_bundle_host_path(manifest_path: &Path, path: &str) -> PathBuf {
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

fn underlay_route_domain(host: &str, label: Option<&str>) -> String {
    let label = label.map(str::trim).unwrap_or_default();
    if label.is_empty() {
        host.to_owned()
    } else {
        format!("{label}.{host}")
    }
}

fn bundle_input_value<'a>(inputs: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a Value> {
    let mut segments = key.split('.');
    let first = segments.next()?;
    let mut current = inputs.get(first)?;
    for segment in segments {
        current = current.as_table()?.get(segment)?;
    }
    Some(current)
}

fn bundle_input_paths(inputs: &BTreeMap<String, Value>) -> Vec<String> {
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

fn insert_bundle_input_value(inputs: &mut BTreeMap<String, Value>, key: &str, value: Value) {
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

fn validate_bundle_input_type(
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

fn merge_missing_values(current: &mut Value, defaults: &Value) {
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
