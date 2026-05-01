use std::collections::BTreeMap;
use std::path::Path;

use toml::Value;

use super::export::{
    infer_underlay_bundle_source, render_shipped_bundle_template_with_inputs,
    render_toml_string_array, underlay_bootstrap_sync_paths, underlay_dir_or_default,
    underlay_isolated_dirs, underlay_optional_dir_step, underlay_optional_docs_qa_steps,
    underlay_optional_docs_step,
};
use super::{
    bundle_shared_root_path, bundle_source_path, bundle_spec_from_descriptor,
    derive_bundle_workspace_subdir, insert_bundle_input_value, optional_bundle_integer,
    optional_bundle_string, parse_bundle_descriptor_source, render_toml_string_array_lines,
    render_toml_string_list, required_bundle_string, BundleSpec, ManifestError,
};

const DECODELABS_BUNDLE_DESCRIPTOR: &str = include_str!("../../bundles/decodelabs/bundle.toml");
const DECODELABS_TEMPLATE: &str = include_str!("../../bundles/decodelabs/export.toml");
const DECODELABS_LIBRARY_BUNDLE_DESCRIPTOR: &str =
    include_str!("../../bundles/decodelabs-library/bundle.toml");
const DECODELABS_LIBRARY_TEMPLATE: &str =
    include_str!("../../bundles/decodelabs-library/export.toml");
const UNDERLAY_BUNDLE_DESCRIPTOR: &str = include_str!("../../bundles/underlay/bundle.toml");
const UNDERLAY_TEMPLATE: &str = include_str!("../../bundles/underlay/export.toml");

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
    let zest_port = optional_bundle_integer(inputs, "zest_port");
    let zest_domain =
        optional_bundle_string(inputs, "zest_domain").unwrap_or_else(|| format!("zest.{host}"));
    let mut render_inputs = inputs.clone();
    render_inputs.insert("host".to_owned(), Value::String(host));
    render_inputs.insert("project_name".to_owned(), Value::String(project_name));
    render_inputs.insert("database".to_owned(), Value::String(database));
    render_inputs.insert("system_name".to_owned(), Value::String(system_name));
    render_inputs.insert("container_name".to_owned(), Value::String(container_name));
    render_inputs.insert(
        "workspace_service_name".to_owned(),
        Value::String(workspace_service_name),
    );
    render_inputs.insert(
        "default_workspace".to_owned(),
        Value::String(default_workspace),
    );
    insert_bundle_input_value(
        &mut render_inputs,
        "routes.front",
        Value::String(optional_bundle_string(inputs, "routes.front").unwrap_or_default()),
    );
    insert_bundle_input_value(
        &mut render_inputs,
        "routes.admin",
        Value::String(
            optional_bundle_string(inputs, "routes.admin").unwrap_or_else(|| "admin".to_owned()),
        ),
    );
    insert_bundle_input_value(
        &mut render_inputs,
        "routes.api",
        Value::String(
            optional_bundle_string(inputs, "routes.api").unwrap_or_else(|| "api".to_owned()),
        ),
    );
    if let Some(port) = zest_port {
        let port = decodelabs_zest_port(port)?;
        render_inputs.insert("zest_port".to_owned(), Value::Integer(i64::from(port)));
    }
    render_inputs.insert("zest_domain".to_owned(), Value::String(zest_domain));

    let rendered = render_shipped_bundle_template_with_inputs(
        manifest_path,
        "decodelabs",
        &DECODELABS_TEMPLATE.replace(
            "__PHP_EXTENSIONS__",
            &render_toml_string_array_lines(DECODELABS_PHP_EXTENSIONS, "  "),
        ),
        &render_inputs,
    )?;

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("decodelabs"),
        error,
    })
}

fn decodelabs_zest_port(port: i64) -> Result<u16, ManifestError> {
    if port <= 0 {
        return Err(ManifestError::Render {
            path: bundle_source_path("decodelabs"),
            detail: format!(
                "invalid `decodelabs` bundle input `zest_port = {port}`; expected a port in the range 1-65535"
            ),
        });
    }
    u16::try_from(port).map_err(|_| ManifestError::Render {
        path: bundle_source_path("decodelabs"),
        detail: format!(
            "invalid `decodelabs` bundle input `zest_port = {port}`; expected a port in the range 1-65535"
        ),
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
    let mut render_inputs = inputs.clone();
    render_inputs.insert(
        "shared_root".to_owned(),
        Value::String(shared_root_mount.display().to_string()),
    );
    render_inputs.insert(
        "workspace_subdir".to_owned(),
        Value::String(workspace_subdir),
    );
    render_inputs.insert("project_name".to_owned(), Value::String(project_name));
    render_inputs.insert("system_name".to_owned(), Value::String(system_name));
    render_inputs.insert("container_name".to_owned(), Value::String(container_name));
    render_inputs.insert(
        "workspace_service_name".to_owned(),
        Value::String(workspace_service_name),
    );
    render_inputs.insert(
        "default_workspace".to_owned(),
        Value::String(default_workspace),
    );

    let rendered = render_shipped_bundle_template_with_inputs(
        manifest_path,
        "decodelabs-library",
        &DECODELABS_LIBRARY_TEMPLATE.replace(
            "__PHP_EXTENSIONS__",
            &render_toml_string_array_lines(DECODELABS_PHP_EXTENSIONS, "  "),
        ),
        &render_inputs,
    )?;

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
    let bootstrap_sync_paths = underlay_bootstrap_sync_paths(inputs, &underlay_source);
    let bootstrap_sync_command = format!("bootstrap deps sync {}", bootstrap_sync_paths.join(" "));
    let mut render_inputs = inputs.clone();
    render_inputs.insert("host".to_owned(), Value::String(host));
    render_inputs.insert("project_name".to_owned(), Value::String(project_name));
    render_inputs.insert(
        "workspace_subdir".to_owned(),
        Value::String(workspace_subdir),
    );
    render_inputs.insert("database".to_owned(), Value::String(database));
    render_inputs.insert("api_port".to_owned(), Value::Integer(api_port));
    render_inputs.insert("admin_port".to_owned(), Value::Integer(admin_port));
    render_inputs.insert("front_port".to_owned(), Value::Integer(front_port));
    render_inputs.insert("system_name".to_owned(), Value::String(system_name));
    render_inputs.insert("container_name".to_owned(), Value::String(container_name));
    render_inputs.insert(
        "workspace_service_name".to_owned(),
        Value::String(workspace_service_name),
    );
    render_inputs.insert(
        "default_workspace".to_owned(),
        Value::String(default_workspace),
    );
    let rendered = render_shipped_bundle_template_with_inputs(
        manifest_path,
        "underlay",
        &UNDERLAY_TEMPLATE
            .replace(
                "__DATABASES__",
                &render_toml_string_list(inputs, "databases"),
            )
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
            .replace(
                "__ISOLATED_DIRS__",
                &render_toml_string_array(&underlay_isolated_dirs(inputs)),
            ),
        &render_inputs,
    )?;

    toml::from_str::<Value>(&rendered).map_err(|error| ManifestError::Parse {
        path: bundle_source_path("underlay"),
        error,
    })
}
