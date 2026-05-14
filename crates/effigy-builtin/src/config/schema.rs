use super::super::doc_render::{append_doc_lines, render_prefixed_doc};
use super::super::text_doc::TextDoc;
use super::docs::{self, ConfigDocProfile};
use super::request::{ConfigSchemaTarget, ConfigTestRunner};
use effigy_manifest::{BundleInputSpec, BundleInputType, BundleSpec};

const HEADER_CANONICAL: &str = "# Canonical strict-valid effigy.toml schema template";
const HEADER_MINIMAL: &str = "# Minimal strict-valid effigy.toml starter";

pub(super) fn render_builtin_config_schema() -> String {
    let mut doc = TextDoc::new();
    doc.line(HEADER_CANONICAL);
    doc.blank();
    append_doc_lines(&mut doc, docs::manifest_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::distribution_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::containers_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::demos_lines(ConfigDocProfile::Schema));
    append_doc_lines(
        &mut doc,
        docs::package_manager_lines(ConfigDocProfile::Schema),
    );
    append_doc_lines(
        &mut doc,
        docs::test_section_lines(true, ConfigDocProfile::Schema, None),
    );
    append_doc_lines(&mut doc, docs::secrets_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::state_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::deploy_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::defer_lines().iter().copied());
    append_doc_lines(&mut doc, docs::shell_lines().iter().copied());
    append_doc_lines(&mut doc, docs::scan_lines().iter().copied());
    append_doc_lines(
        &mut doc,
        docs::tasks_canonical_lines(ConfigDocProfile::Schema),
    );
    doc.finish()
}

pub(super) fn render_builtin_config_schema_minimal() -> String {
    let mut doc = TextDoc::new();
    doc.line(HEADER_MINIMAL);
    doc.blank();
    append_doc_lines(&mut doc, docs::manifest_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::distribution_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::containers_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::demos_lines(ConfigDocProfile::Schema));
    append_doc_lines(
        &mut doc,
        docs::package_manager_lines(ConfigDocProfile::Schema),
    );
    append_doc_lines(
        &mut doc,
        docs::test_section_lines(false, ConfigDocProfile::Schema, Some("vitest")),
    );
    append_doc_lines(&mut doc, docs::tasks_minimal_lines().iter().copied());
    doc.finish()
}

pub(super) fn render_builtin_config_schema_target(
    target: ConfigSchemaTarget,
    minimal: bool,
) -> String {
    if target == ConfigSchemaTarget::Bundle {
        return render_builtin_config_schema_bundle_target(minimal, None, &[]);
    }

    render_prefixed_doc(
        &format!(
            "{} ({} target)",
            schema_header_prefix(minimal),
            target.as_str()
        ),
        target_schema_lines(target, minimal),
    )
}

pub(super) fn render_builtin_config_schema_bundle_target(
    minimal: bool,
    bundle: Option<&BundleSpec>,
    default_paths: &[String],
) -> String {
    let header = match bundle {
        Some(bundle) => format!(
            "{} (bundle target, bundle: {})",
            schema_header_prefix(minimal),
            bundle.name
        ),
        None => format!("{} (bundle target)", schema_header_prefix(minimal)),
    };

    let mut doc = TextDoc::new();
    doc.line(header);
    doc.blank();
    for line in bundle_schema_lines(bundle, default_paths) {
        doc.line(line);
    }
    doc.finish()
}

pub(super) fn render_builtin_config_schema_test_target(
    minimal: bool,
    runner: Option<ConfigTestRunner>,
) -> String {
    let header = match (minimal, runner) {
        (true, Some(name)) => {
            format!(
                "# Minimal strict-valid effigy.toml starter (test target, runner: {})",
                name.as_str()
            )
        }
        (true, None) => "# Minimal strict-valid effigy.toml starter (test target)".to_owned(),
        (false, Some(name)) => {
            format!(
                "# Canonical strict-valid effigy.toml schema template (test target, runner: {})",
                name.as_str()
            )
        }
        (false, None) => {
            "# Canonical strict-valid effigy.toml schema template (test target)".to_owned()
        }
    };

    let mut doc = TextDoc::new();
    doc.line(header);
    doc.blank();
    append_doc_lines(
        &mut doc,
        docs::test_section_lines(
            !minimal,
            ConfigDocProfile::Schema,
            runner.map(ConfigTestRunner::as_str),
        ),
    );
    doc.finish()
}

fn schema_header_prefix(minimal: bool) -> &'static str {
    if minimal {
        HEADER_MINIMAL
    } else {
        HEADER_CANONICAL
    }
}

fn target_schema_lines(
    target: ConfigSchemaTarget,
    minimal: bool,
) -> Box<dyn Iterator<Item = &'static str>> {
    match target {
        ConfigSchemaTarget::Manifest => {
            Box::new(docs::manifest_lines(ConfigDocProfile::Schema).into_iter())
        }
        ConfigSchemaTarget::Bundle => unreachable!("bundle target is rendered dynamically"),
        ConfigSchemaTarget::Demos => {
            Box::new(docs::demos_lines(ConfigDocProfile::Schema).into_iter())
        }
        ConfigSchemaTarget::PackageManager => {
            Box::new(docs::package_manager_lines(ConfigDocProfile::Schema).into_iter())
        }
        ConfigSchemaTarget::Test => {
            Box::new(docs::test_section_lines(!minimal, ConfigDocProfile::Schema, None).into_iter())
        }
        ConfigSchemaTarget::Tasks if minimal => {
            Box::new(docs::tasks_minimal_lines().iter().copied())
        }
        ConfigSchemaTarget::Tasks => {
            Box::new(docs::tasks_canonical_lines(ConfigDocProfile::Schema).into_iter())
        }
        ConfigSchemaTarget::Defer => Box::new(docs::defer_lines().iter().copied()),
        ConfigSchemaTarget::Scan => Box::new(docs::scan_lines().iter().copied()),
        ConfigSchemaTarget::Shell => Box::new(docs::shell_lines().iter().copied()),
    }
}

fn bundle_schema_lines(bundle: Option<&BundleSpec>, default_paths: &[String]) -> Vec<String> {
    let mut lines = vec![
        "[bundle]".to_owned(),
        "# Bundle base selects a local or git-hosted preset.".to_owned(),
    ];
    match bundle {
        Some(b) => lines.push(format!("base = \"{}\"", b.name)),
        None => lines.push("# base = { type = \"path\", dir = \"bundles/acme\" }".to_owned()),
    };
    lines.push("# To import a repo-local bundle directory instead, set `base = { type = \"path\", dir = \"bundles/acme\" }`.".to_owned());
    lines.push("# Local bundle directories contain `bundle.toml` metadata plus an `effigy.toml` defaults template under that `dir`.".to_owned());
    lines.push(
        "# Local bundle templates can reference bundled scripts and assets with `{{ bundle.root }}`."
            .to_owned(),
    );
    lines.push(
        "# Repo-owned run steps can also reference the active bundle root with `{{ bundle.root }}`."
            .to_owned(),
    );
    lines.push("# All other keys are bundle-defined inputs.".to_owned());

    match bundle {
        Some(bundle) => {
            lines.push(format!("# {}", bundle.description));
            lines.push(String::new());
            for input in &bundle.inputs {
                lines.extend(render_bundle_input_lines(input));
            }

            if !default_paths.is_empty() {
                lines.push(String::new());
                lines.push("# Default paths populated by this bundle:".to_owned());
                for path in default_paths {
                    lines.push(format!("# - {path}"));
                }
            }
        }
        None => {
            lines.push(
                "# Use `effigy bundle inspect` to inspect the active repo bundle source."
                    .to_owned(),
            );
            lines.push(
                "# Use `effigy bundle sync` to refresh remote git or OCI sources.".to_owned(),
            );
        }
    }

    lines
}

fn render_bundle_input_lines(input: &BundleInputSpec) -> Vec<String> {
    let mut lines = vec![format!(
        "# {} [{}{}]",
        input.description,
        render_bundle_input_type(input.value_type),
        if input.required {
            ", required"
        } else {
            ", optional"
        }
    )];

    if let Some(example) = &input.example {
        lines.push(render_bundle_value_line(&input.name, example));
    } else if let Some(default) = &input.default {
        lines.push(render_bundle_value_line(&input.name, default));
    } else {
        lines.push(render_bundle_placeholder_line(
            &input.name,
            input.value_type,
        ));
    }

    lines
}

fn render_bundle_input_type(value_type: BundleInputType) -> &'static str {
    match value_type {
        BundleInputType::String => "string",
        BundleInputType::Integer => "integer",
        BundleInputType::Bool => "bool",
        BundleInputType::List => "list",
    }
}

fn render_bundle_value_line(name: &str, value: &toml::Value) -> String {
    format!("{name} = {}", value)
}

fn render_bundle_placeholder_line(name: &str, value_type: BundleInputType) -> String {
    let value = match value_type {
        BundleInputType::String => "\"value\"",
        BundleInputType::Integer => "1",
        BundleInputType::Bool => "true",
        BundleInputType::List => "[]",
    };

    format!("{name} = {value}")
}
