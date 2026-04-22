use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleInputType {
    String,
    Integer,
    Bool,
    List,
}

pub(crate) fn apply_bundle_defaults(
    manifest_path: &Path,
    current: &mut Value,
) -> Result<Option<String>, ManifestError> {
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

    let defaults = resolve_bundle_defaults(manifest_path, &bundle.name, &bundle.inputs)?;
    merge_missing_values(current, &defaults);
    Ok(Some(bundle.name))
}

pub(crate) fn bundle_source_path(name: &str) -> PathBuf {
    PathBuf::from(format!("<bundle:{name}>"))
}

pub fn list_bundles() -> Vec<BundleSpec> {
    vec![decodelabs_spec()]
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

fn resolve_bundle_defaults(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    match bundle_name {
        "decodelabs" => resolve_decodelabs_bundle(manifest_path, inputs),
        other => Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("unknown bundle `{other}`"),
        }),
    }
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
                required: true,
                description: "Default MariaDB database name for the app and bundled db alias rendering.".to_owned(),
                default: None,
                example: Some(Value::String("contactpatch".to_owned())),
            },
        ],
    }
}

fn resolve_decodelabs_bundle(
    manifest_path: &Path,
    inputs: &BTreeMap<String, Value>,
) -> Result<Value, ManifestError> {
    let host = required_bundle_string(manifest_path, "decodelabs", inputs, "host")?;
    let project_name = required_bundle_string(manifest_path, "decodelabs", inputs, "project_name")?;
    let database = required_bundle_string(manifest_path, "decodelabs", inputs, "database")?;

    let template = r#"
[containers]
default = "web"

[containers.web]
driver = "colima"
startup = "detached"
project_name = "__PROJECT_NAME__"
primary_service = "app"
working_dir = "/var/www/html"

[containers.web.lifecycle]
on_task_exit = "stop"
shutdown = "graceful"

[containers.web.dns]
routes = [
  { domain = "__HOST__", tls = true, service = "web" },
  { domain = "pma.__HOST__", tls = true, service = "pma" },
]

[containers.web.aliases]
php = "app"
composer = { service = "app", command = "composer" }
mysql = { service = "db", command = "mysql -uroot{% if services.db.params.root_password %} -p{{ services.db.params.root_password }}{% endif %} {{ services.db.params.database }}" }

[containers.web.services.app]
catalog = "php-fpm"
version = "8.4"
document_root = "."
node_version = "20"
node_global_packages = ["eclint"]
composer_global_packages = ["decodelabs/effigy"]
extensions = [
  "pdo_mysql",
  "intl",
  "exif",
  "zip",
  "gd",
  "redis",
  "memcached",
  "opcache",
]

[containers.web.services.web]
catalog = "nginx"
document_root = "."
rewrite_all_to = "/vendor/genesis.php"
asset_fallback = ""
error_page_404 = "/vendor/genesis.php"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"
database = "__DATABASE__"

[containers.web.services.pma]
catalog = "phpmyadmin"
version = "latest"
database_host = "db"

[containers.web.services.memcache]
catalog = "memcached"
memory = 128

[containers.web.services.redis]
catalog = "redis"
version = "7"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[tasks.dev]
workspace = "app"
"#;

    let rendered = template
        .replace("__HOST__", &host)
        .replace("__PROJECT_NAME__", &project_name)
        .replace("__DATABASE__", &database);

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("decodelabs"),
        error,
    })
}

fn required_bundle_string(
    manifest_path: &Path,
    bundle_name: &str,
    inputs: &BTreeMap<String, Value>,
    key: &str,
) -> Result<String, ManifestError> {
    let Some(value) = inputs.get(key) else {
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
