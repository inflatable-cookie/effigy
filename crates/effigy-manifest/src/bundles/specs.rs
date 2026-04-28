use std::collections::BTreeMap;
use std::path::Path;

use toml::Value;

use super::export::{
    infer_underlay_bundle_source, materialize_shipped_bundle_assets,
    render_shipped_bundle_template, render_toml_string_array, underlay_bootstrap_sync_paths,
    underlay_cargo_target_dirs, underlay_node_modules_dirs,
};
use super::{
    bundle_shared_root_path, bundle_source_path, derive_bundle_workspace_subdir,
    optional_bundle_integer, optional_bundle_string, render_toml_string_array_lines,
    render_toml_string_list, required_bundle_string, underlay_route_domain, BundleInputSpec,
    BundleInputType, BundleSpec, ManifestError,
};

pub(super) fn decodelabs_spec() -> BundleSpec {
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

pub(super) fn decodelabs_library_spec() -> BundleSpec {
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
                description: "Compose project name for the shared library container runtime. Defaults to a repo-specific name derived from `workspace_subdir`.".to_owned(),
                default: None,
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

pub(super) const DECODELABS_PHP_EXTENSIONS: &[&str] = &[
    "bcmath",
    "apcu",
    "bz2",
    "calendar",
    "curl",
    "gmp",
    "imagick",
    "mbstring",
    "pcntl",
    "exif",
    "gd",
    "intl",
    "memcached",
    "mysqli",
    "opcache",
    "pdo_mysql",
    "readline",
    "redis",
    "sockets",
    "sqlite3",
    "xml",
    "zip",
    "event",
];

pub(super) fn resolve_decodelabs_bundle(
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
mysql = { service = "db", command = "mysql -uroot{% if services.db.params.password %} -p{{ services.db.params.password }}{% endif %} {{ services.db.params.database }}" }

[containers.__CONTAINER_NAME__.services.__WORKSPACE_SERVICE_NAME__]
catalog = "php-fpm"
version = "8.4"
document_root = "."
isolated_dirs = ["vendor", "node_modules"]
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
stay_in_shell = true
run_in = "container"
run = [{ rhai = "{{ bundle.root }}/scripts/seed-latest-db-dump.rhai" }]

[tasks.release]
run = "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" release"

[defer]
run = "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" {request} {args}"
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

pub(super) fn resolve_decodelabs_library_bundle(
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
        .unwrap_or_else(|| default_decodelabs_library_project_name(&workspace_subdir));
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
working_dir = "/workspace-root/__WORKSPACE_SUBDIR__"
mount_source = "__SHARED_ROOT__"
isolated_dirs = ["vendor"]
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
run = "\"${COMPOSER_HOME:-$HOME/.config/composer}/vendor/bin/effigy\" {request} {args}"
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

pub(super) fn default_decodelabs_library_project_name(workspace_subdir: &str) -> String {
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
        "decodelabs-library-dev".to_owned()
    } else {
        format!("decodelabs-library-{slug}-dev")
    }
}

pub(super) fn underlay_spec() -> BundleSpec {
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
                name: "dirs.api".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "API package directory for bootstrap env generation and dependency sync. When omitted, the bundle falls back to the shipped Underlay guesses.".to_owned(),
                default: None,
                example: Some(Value::String("farmyard".to_owned())),
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
                name: "sources.underlay".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Relative path from the consumer repo to the sibling underlay checkout used by bootstrap sync, bootstrap children, and the bundled UI setup helper.".to_owned(),
                default: Some(Value::String("../underlay".to_owned())),
                example: Some(Value::String("../../underlay".to_owned())),
            },
            BundleInputSpec {
                name: "sources.poodle".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Relative path from the consumer repo to the sibling poodle checkout used by bootstrap children and the bundled UI setup helper.".to_owned(),
                default: Some(Value::String("../poodle".to_owned())),
                example: Some(Value::String("../../poodle".to_owned())),
            },
            BundleInputSpec {
                name: "dirs.client".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Shared client package directory the bundled UI setup helper should hydrate before front/admin startup. When omitted, the helper falls back to the shipped Underlay guesses.".to_owned(),
                default: None,
                example: Some(Value::String("froyo".to_owned())),
            },
            BundleInputSpec {
                name: "dirs.ui".to_owned(),
                value_type: BundleInputType::String,
                required: false,
                description: "Optional UI package directory to include in bootstrap dependency sync and UI hydration. When omitted, the helper falls back to the shipped Underlay guesses.".to_owned(),
                default: None,
                example: Some(Value::String("app-ui".to_owned())),
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

pub(super) fn resolve_underlay_bundle(
    manifest_path: &Path,
    current: &Value,
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
    let underlay_source = infer_underlay_bundle_source(
        current,
        &system_name,
        optional_bundle_string(inputs, "sources.underlay"),
        "underlay",
        "../underlay",
    );
    let poodle_source = infer_underlay_bundle_source(
        current,
        &system_name,
        optional_bundle_string(inputs, "sources.poodle"),
        "poodle",
        "../poodle",
    );
    let bundle_root = materialize_shipped_bundle_assets(manifest_path, "underlay")?;
    let bootstrap_sync_paths = underlay_bootstrap_sync_paths(inputs, &underlay_source);
    let bootstrap_sync_command = format!("bootstrap deps sync {}", bootstrap_sync_paths.join(" "));
    let cargo_target_dirs = render_toml_string_array(&underlay_cargo_target_dirs(inputs));
    let node_modules_dirs = render_toml_string_array(&underlay_node_modules_dirs(inputs));

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
cargo_target_dirs = __CARGO_TARGET_DIRS__
node_modules_dirs = __NODE_MODULES_DIRS__

[containers.__CONTAINER_NAME__.services.postgres]
catalog = "postgres"
database = "__DATABASE__"
databases = __DATABASES__

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

[bootstrap]
run = [
  { rhai = "{{ bundle.root }}/scripts/bootstrap-env.rhai" },
  { task = "container up --detach" },
  { task = "__BOOTSTRAP_SYNC_COMMAND__" },
]
start = "dev"

[[bootstrap.children]]
path = "__UNDERLAY_SOURCE__"
repo = "git@github.com:inflatable-cookie/underlay.git"
branch = "main"

[[bootstrap.children]]
path = "__POODLE_SOURCE__"
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
            .replace("__UNDERLAY_SOURCE__", &underlay_source)
            .replace("__POODLE_SOURCE__", &poodle_source)
            .replace("__BOOTSTRAP_SYNC_COMMAND__", &bootstrap_sync_command)
            .replace("__CARGO_TARGET_DIRS__", &cargo_target_dirs)
            .replace("__NODE_MODULES_DIRS__", &node_modules_dirs)
            .replace("__DEFAULT_WORKSPACE__", &default_workspace),
    )?;

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("underlay"),
        error,
    })
}
