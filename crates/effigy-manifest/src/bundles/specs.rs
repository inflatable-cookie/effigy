use std::collections::BTreeMap;
use std::path::Path;

use toml::Value;

use super::export::{
    infer_underlay_bundle_source, materialize_shipped_bundle_assets,
    render_shipped_bundle_template, render_toml_string_array, underlay_bootstrap_sync_paths,
    underlay_dir_or_default, underlay_isolated_dirs, underlay_optional_dir_step,
    underlay_optional_docs_qa_steps, underlay_optional_docs_step,
};
use super::{
    bundle_shared_root_path, bundle_source_path, bundle_spec_from_descriptor,
    derive_bundle_workspace_subdir, optional_bundle_integer, optional_bundle_string,
    parse_bundle_descriptor_source, render_toml_string_array_lines, render_toml_string_list,
    required_bundle_string, underlay_route_domain, BundleSpec, ManifestError,
};

const DECODELABS_BUNDLE_DESCRIPTOR: &str = include_str!("../../bundles/decodelabs/bundle.toml");
const DECODELABS_DEFAULTS_TEMPLATE: &str = include_str!("../../bundles/decodelabs/defaults.toml");
const DECODELABS_LIBRARY_BUNDLE_DESCRIPTOR: &str =
    include_str!("../../bundles/decodelabs-library/bundle.toml");
const DECODELABS_LIBRARY_DEFAULTS_TEMPLATE: &str =
    include_str!("../../bundles/decodelabs-library/defaults.toml");
const UNDERLAY_BUNDLE_DESCRIPTOR: &str = include_str!("../../bundles/underlay/bundle.toml");
const UNDERLAY_DEFAULTS_TEMPLATE: &str = include_str!("../../bundles/underlay/defaults.toml");

pub(super) fn decodelabs_spec() -> BundleSpec {
    let path = bundle_source_path("decodelabs");
    let descriptor = parse_bundle_descriptor_source(&path, DECODELABS_BUNDLE_DESCRIPTOR)
        .expect("embedded decodelabs bundle descriptor must parse");
    bundle_spec_from_descriptor(&descriptor)
}

pub(super) fn decodelabs_library_spec() -> BundleSpec {
    let path = bundle_source_path("decodelabs-library");
    let descriptor = parse_bundle_descriptor_source(&path, DECODELABS_LIBRARY_BUNDLE_DESCRIPTOR)
        .expect("embedded decodelabs-library bundle descriptor must parse");
    bundle_spec_from_descriptor(&descriptor)
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

    let rendered = DECODELABS_DEFAULTS_TEMPLATE
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

    let rendered = DECODELABS_LIBRARY_DEFAULTS_TEMPLATE
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
    let path = bundle_source_path("underlay");
    let descriptor = parse_bundle_descriptor_source(&path, UNDERLAY_BUNDLE_DESCRIPTOR)
        .expect("embedded underlay bundle descriptor must parse");
    bundle_spec_from_descriptor(&descriptor)
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
    let isolated_dirs = render_toml_string_array(&underlay_isolated_dirs(inputs));

    let rendered = render_shipped_bundle_template(
        manifest_path,
        "underlay",
        &bundle_root,
        &UNDERLAY_DEFAULTS_TEMPLATE
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
            .replace(
                "__DOCS_HEALTH__",
                &underlay_optional_docs_step(inputs, "health"),
            )
            .replace(
                "__DOCS_VALIDATE__",
                &underlay_optional_docs_step(inputs, "validate"),
            )
            .replace("__DOCS_QA__", &underlay_optional_docs_qa_steps(inputs))
            .replace(
                "__API_DIR__",
                &underlay_dir_or_default(inputs, "dirs.api", "app-api"),
            )
            .replace(
                "__CLIENT_DIR__",
                &underlay_dir_or_default(inputs, "dirs.client", "app-client"),
            )
            .replace(
                "__UI_HEALTH__",
                &underlay_optional_dir_step(inputs, "dirs.ui", "app-ui", "health"),
            )
            .replace(
                "__UI_VALIDATE__",
                &underlay_optional_dir_step(inputs, "dirs.ui", "app-ui", "validate"),
            )
            .replace(
                "__ADMIN_DIR__",
                &underlay_dir_or_default(inputs, "dirs.admin", "app-admin"),
            )
            .replace(
                "__FRONT_DIR__",
                &underlay_dir_or_default(inputs, "dirs.front", "app-front"),
            )
            .replace("__ISOLATED_DIRS__", &isolated_dirs)
            .replace("__DEFAULT_WORKSPACE__", &default_workspace),
    )?;

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("underlay"),
        error,
    })
}
