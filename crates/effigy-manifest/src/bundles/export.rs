use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Value;

use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;

use super::specs::DECODELABS_PHP_EXTENSIONS;
use super::{
    bundle_source_path, get_bundle, optional_bundle_string, render_toml_string_array_lines,
    BundleInputType, BundleSpec, LocalBundleTemplateBundle, ManifestError,
};

pub(super) fn render_shipped_bundle_template(
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
pub(super) struct EmbeddedBundleAsset {
    pub(super) path: &'static str,
    pub(super) contents: &'static str,
}

pub(super) struct BundleExportFile {
    pub(super) path: &'static str,
    pub(super) contents: String,
}

const DECODELABS_ASSETS: &[EmbeddedBundleAsset] = &[EmbeddedBundleAsset {
    path: "scripts/seed-latest-db-dump.rhai",
    contents: include_str!(
        "../../../effigy-catalog/starters/decodelabs/scripts/seed-latest-db-dump.rhai"
    ),
}];

const UNDERLAY_ASSETS: &[EmbeddedBundleAsset] = &[
    EmbeddedBundleAsset {
        path: "scripts/bootstrap-env.rhai",
        contents: include_str!(
            "../../../effigy-catalog/starters/underlay/scripts/bootstrap-env.rhai"
        ),
    },
    EmbeddedBundleAsset {
        path: "scripts/dev/ui-setup.rhai",
        contents: include_str!(
            "../../../effigy-catalog/starters/underlay/scripts/dev/ui-setup.rhai"
        ),
    },
    EmbeddedBundleAsset {
        path: "scripts/error-reporting.rhai",
        contents: include_str!(
            "../../../effigy-catalog/starters/underlay/scripts/error-reporting.rhai"
        ),
    },
];

pub(super) fn shipped_bundle_export_files(
    name: &str,
) -> Result<Vec<BundleExportFile>, ManifestError> {
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

pub(super) fn render_export_descriptor(spec: &BundleSpec) -> String {
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

pub(super) fn render_export_readme(spec: &BundleSpec) -> String {
    format!(
        "# {} bundle\n\n{}\n\nUse from a consuming manifest with:\n\n```toml\n[bundle]\nbase_path = \"path/to/{}\"\n# set the inputs from bundle.toml here\n```\n",
        spec.name, spec.description, spec.name
    )
}

pub(super) fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

pub(super) fn bundle_input_type_literal(value_type: BundleInputType) -> &'static str {
    match value_type {
        BundleInputType::String => "string",
        BundleInputType::Integer => "integer",
        BundleInputType::Bool => "bool",
        BundleInputType::List => "list",
    }
}

pub(super) fn decodelabs_export_template() -> String {
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
mysql = {{ service = "db", command = "mysql -uroot{{% raw %}}{{% if services.db.params.password %}} -p{{{{ services.db.params.password }}}}{{% endif %}}{{% endraw %}} {{{{ inputs.databases|first }}}}" }}

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
database = "{{{{ inputs.databases|first }}}}"
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
stay_in_shell = true
run_in = "container"
run = [{{ rhai = "{{{{ bundle.root }}}}/scripts/seed-latest-db-dump.rhai" }}]

[tasks.release]
run = "\"${{COMPOSER_HOME:-$HOME/.config/composer}}/vendor/bin/effigy\" release"

[defer]
run = "\"${{COMPOSER_HOME:-$HOME/.config/composer}}/vendor/bin/effigy\" {{request}} {{args}}"
run_in = "container"
"#
    )
}

pub(super) fn decodelabs_library_export_template() -> String {
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
run = "\"${{COMPOSER_HOME:-$HOME/.config/composer}}/vendor/bin/effigy\" {{request}} {{args}}"
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
isolated_dirs = [
  "{% if inputs.dirs.api %}{{ inputs.dirs.api }}{% else %}app-api{% endif %}/target",
  "{% if inputs.dirs.client %}{{ inputs.dirs.client }}{% else %}app-client{% endif %}/node_modules",
  "{% if inputs.dirs.ui %}{{ inputs.dirs.ui }}{% else %}app-ui{% endif %}/node_modules",
  "{% if inputs.dirs.front %}{{ inputs.dirs.front }}{% else %}app-front{% endif %}/node_modules",
  "{% if inputs.dirs.admin %}{{ inputs.dirs.admin }}{% else %}app-admin{% endif %}/node_modules",
]

[containers.{{ inputs.container_name }}.services.postgres]
catalog = "postgres"
database = "{{ inputs.databases|first }}"
databases = [{% for database in inputs.databases %}"{{ database }}"{% if not loop.last %}, {% endif %}{% endfor %}]

[containers.{{ inputs.container_name }}.services.dbgate]
catalog = "dbgate"
database_host = "postgres"
database = "{{ inputs.databases|first }}"
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

[tasks.health]
run = [
{% if inputs.dirs.docs %}  { task = "{{ inputs.dirs.docs }}/health" },
{% endif %}  { task = "{% if inputs.dirs.api %}{{ inputs.dirs.api }}{% else %}app-api{% endif %}/health" },
  { task = "{% if inputs.dirs.client %}{{ inputs.dirs.client }}{% else %}app-client{% endif %}/health" },
{% if inputs.dirs.ui %}  { task = "{{ inputs.dirs.ui }}/health" },
{% endif %}  { task = "{% if inputs.dirs.admin %}{{ inputs.dirs.admin }}{% else %}app-admin{% endif %}/health" },
  { task = "{% if inputs.dirs.front %}{{ inputs.dirs.front }}{% else %}app-front{% endif %}/health" },
]

[tasks.validate]
run = [
  { task = "underlay/validate" },
{% if inputs.dirs.docs %}  { task = "{{ inputs.dirs.docs }}/validate" },
{% endif %}  { task = "{% if inputs.dirs.api %}{{ inputs.dirs.api }}{% else %}app-api{% endif %}/validate" },
  { task = "{% if inputs.dirs.client %}{{ inputs.dirs.client }}{% else %}app-client{% endif %}/validate" },
{% if inputs.dirs.ui %}  { task = "{{ inputs.dirs.ui }}/validate" },
{% endif %}  { task = "{% if inputs.dirs.admin %}{{ inputs.dirs.admin }}{% else %}app-admin{% endif %}/validate" },
  { task = "{% if inputs.dirs.front %}{{ inputs.dirs.front }}{% else %}app-front{% endif %}/validate" },
]

[tasks.qa]
run = [
  { task = "health" },
  { task = "validate" },
{% if inputs.dirs.docs %}  { task = "{{ inputs.dirs.docs }}/qa:docs" },
  { task = "{{ inputs.dirs.docs }}/qa:northstar" },
{% endif %}]

[tasks.dev]
mode = "tui"
container_lifecycle = true
gateway = true
health_wait = true
concurrent = [
  { name = "front", task = "{% if inputs.dirs.front %}{{ inputs.dirs.front }}{% else %}app-front{% endif %}/dev", setup = [{ rhai = "{{ bundle.root }}/scripts/dev/ui-setup.rhai" }], start = 6, tab = 1 },
  { name = "admin", task = "{% if inputs.dirs.admin %}{{ inputs.dirs.admin }}{% else %}app-admin{% endif %}/dev", setup = [{ rhai = "{{ bundle.root }}/scripts/dev/ui-setup.rhai" }], start = 5, tab = 2 },
  { name = "api", task = "{% if inputs.dirs.api %}{{ inputs.dirs.api }}{% else %}app-api{% endif %}/api", start = 4, tab = 3 },
  { name = "jobs", task = "{% if inputs.dirs.api %}{{ inputs.dirs.api }}{% else %}app-api{% endif %}/jobs", start = 3, tab = 4, start_after_ms = 1500 },
  { role = "shell", service = "{{ inputs.workspace_service_name }}", start = 2, tab = 5 },
  { role = "lifecycle", start = 1, tab = 6 },
]

[bootstrap]
run = [
  { rhai = "{{ bundle.root }}/scripts/bootstrap-env.rhai" },
  { task = "container up --detach" },
  { task = "bootstrap deps sync {% if inputs.sources.underlay %}{{ inputs.sources.underlay }}{% else %}../underlay{% endif %}{% if inputs.dirs.api %} {{ inputs.dirs.api }}{% else %} app-api{% endif %}{% if inputs.dirs.client %} {{ inputs.dirs.client }}{% else %} app-client{% endif %}{% if inputs.dirs.ui %} {{ inputs.dirs.ui }}{% else %} app-ui{% endif %}{% if inputs.dirs.front %} {{ inputs.dirs.front }}{% else %} app-front{% endif %}{% if inputs.dirs.admin %} {{ inputs.dirs.admin }}{% else %} app-admin{% endif %}" },
]
start = "dev"

[[bootstrap.children]]
path = "{% if inputs.sources.underlay %}{{ inputs.sources.underlay }}{% else %}../underlay{% endif %}"
repo = "git@github.com:inflatable-cookie/underlay.git"
branch = "main"

[[bootstrap.children]]
path = "{% if inputs.sources.poodle %}{{ inputs.sources.poodle }}{% else %}../poodle{% endif %}"
repo = "git@github.com:inflatable-cookie/poodle.git"
branch = "main"

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

pub(super) fn underlay_export_template() -> String {
    UNDERLAY_EXPORT_TEMPLATE.to_owned()
}

/// Derive isolated writable directories for the underlay workspace bundle from
/// `[bundle.dirs]`. The public surface is a single `isolated_dirs` list whose
/// entries are relative to the workspace `working_dir`.
pub(super) fn underlay_isolated_dirs(inputs: &BTreeMap<String, Value>) -> Vec<String> {
    let mut out = Vec::new();

    let api = optional_bundle_string(inputs, "dirs.api").unwrap_or_else(|| "app-api".to_owned());
    let api = api.trim();
    if !api.is_empty() {
        out.push(format!("{api}/target"));
    }

    for value in [
        optional_bundle_string(inputs, "dirs.client").unwrap_or_else(|| "app-client".to_owned()),
        optional_bundle_string(inputs, "dirs.ui").unwrap_or_else(|| "app-ui".to_owned()),
        optional_bundle_string(inputs, "dirs.front").unwrap_or_else(|| "app-front".to_owned()),
        optional_bundle_string(inputs, "dirs.admin").unwrap_or_else(|| "app-admin".to_owned()),
    ] {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let isolated = format!("{trimmed}/node_modules");
        if !out.contains(&isolated) {
            out.push(isolated);
        }
    }

    out
}

pub(super) fn underlay_dir_or_default(
    inputs: &BTreeMap<String, Value>,
    path: &str,
    fallback: &str,
) -> String {
    optional_bundle_string(inputs, path).unwrap_or_else(|| fallback.to_owned())
}

pub(super) fn underlay_optional_dir_step(
    inputs: &BTreeMap<String, Value>,
    path: &str,
    fallback: &str,
    task: &str,
) -> String {
    let dir = optional_bundle_string(inputs, path).unwrap_or_else(|| fallback.to_owned());
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("  {{ task = \"{trimmed}/{task}\" }},\n")
    }
}

pub(super) fn underlay_optional_docs_step(inputs: &BTreeMap<String, Value>, task: &str) -> String {
    let Some(dir) = optional_bundle_string(inputs, "dirs.docs") else {
        return String::new();
    };
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("  {{ task = \"{trimmed}/{task}\" }},\n")
    }
}

pub(super) fn underlay_optional_docs_qa_steps(inputs: &BTreeMap<String, Value>) -> String {
    let Some(dir) = optional_bundle_string(inputs, "dirs.docs") else {
        return String::new();
    };
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!(
            "  {{ task = \"{trimmed}/qa:docs\" }},\n  {{ task = \"{trimmed}/qa:northstar\" }},\n"
        )
    }
}

pub(super) fn render_toml_string_array(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".to_owned();
    }
    let encoded = values
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{encoded}]")
}

pub(super) fn underlay_bootstrap_sync_paths(
    inputs: &BTreeMap<String, Value>,
    underlay_source: &str,
) -> Vec<String> {
    let mut paths = vec![underlay_source.trim().to_owned()];
    let candidates = [
        optional_bundle_string(inputs, "dirs.api").unwrap_or_else(|| "app-api".to_owned()),
        optional_bundle_string(inputs, "dirs.client").unwrap_or_else(|| "app-client".to_owned()),
        optional_bundle_string(inputs, "dirs.ui").unwrap_or_else(|| "app-ui".to_owned()),
        optional_bundle_string(inputs, "dirs.front").unwrap_or_else(|| "app-front".to_owned()),
        optional_bundle_string(inputs, "dirs.admin").unwrap_or_else(|| "app-admin".to_owned()),
    ];

    for candidate in candidates {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = trimmed.to_owned();
        if !paths.contains(&value) {
            paths.push(value);
        }
    }

    paths
}

pub(super) fn infer_underlay_bundle_source(
    current: &Value,
    system_name: &str,
    explicit: Option<String>,
    suffix: &str,
    fallback: &str,
) -> String {
    if let Some(explicit) = explicit {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
    }

    let mounts = current
        .as_table()
        .and_then(|table| table.get("systems"))
        .and_then(Value::as_table)
        .and_then(|systems| systems.get(system_name))
        .and_then(Value::as_table)
        .and_then(|system| system.get("mounts"))
        .and_then(Value::as_array);

    if let Some(mounts) = mounts {
        let expected_suffix = format!("/{suffix}");
        for mount in mounts {
            let Some(mount) = mount.as_str() else {
                continue;
            };
            let trimmed = mount.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == suffix || trimmed.ends_with(&expected_suffix) {
                return trimmed.to_owned();
            }
        }
    }

    fallback.to_owned()
}

pub(super) fn materialize_shipped_bundle_assets(
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

pub(super) fn prune_stale_materialized_bundle_roots(
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

pub(super) fn embedded_bundle_assets(bundle_name: &str) -> &'static [EmbeddedBundleAsset] {
    match bundle_name {
        "decodelabs" => DECODELABS_ASSETS,
        "decodelabs-library" => &[],
        "underlay" => UNDERLAY_ASSETS,
        _ => &[],
    }
}

pub(super) fn is_virtual_bundle_manifest_path(manifest_path: &Path) -> bool {
    manifest_path.to_string_lossy().starts_with("<bundle:")
}

pub(super) fn embedded_bundle_assets_hash(
    bundle_name: &str,
    assets: &[EmbeddedBundleAsset],
) -> String {
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
