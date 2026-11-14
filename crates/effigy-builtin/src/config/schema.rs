use super::super::doc_render::append_doc_lines;
use super::super::text_doc::TextDoc;
use super::docs::{self, ConfigDocProfile};
use super::request::{ConfigSchemaTarget, ConfigTestRunner};
use effigy_manifest::{BundleInputSpec, BundleInputType, BundleSpec};

const HEADER_CANONICAL: &str = "# Canonical strict-valid effigy.toml schema template";
const HEADER_MINIMAL: &str = "# Minimal strict-valid effigy.toml starter";

/// One documented section of the config template.
///
/// The canonical template, the minimal starter and every `--target` view are
/// assembled from these sections, so a section's text is named in exactly one
/// place and a template cannot drift from the target view that shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaSection {
    Manifest,
    DocsIndex,
    DocsPolicyGraph,
    Containers,
    PackageManager,
    Test,
    Secrets,
    State,
    Deploy,
    Defer,
    Scan,
    Tasks,
}

/// The canonical template's sections, in the order it renders them.
const CANONICAL_SECTIONS: &[SchemaSection] = &[
    SchemaSection::Manifest,
    SchemaSection::DocsIndex,
    SchemaSection::DocsPolicyGraph,
    SchemaSection::Containers,
    SchemaSection::PackageManager,
    SchemaSection::Test,
    SchemaSection::Secrets,
    SchemaSection::State,
    SchemaSection::Deploy,
    SchemaSection::Defer,
    SchemaSection::Scan,
    SchemaSection::Tasks,
];

/// The minimal starter's sections: only what a new repository must answer.
const MINIMAL_SECTIONS: &[SchemaSection] = &[
    SchemaSection::Manifest,
    SchemaSection::Containers,
    SchemaSection::PackageManager,
    SchemaSection::Test,
    SchemaSection::Tasks,
];

impl SchemaSection {
    /// One section's lines for a template. `include_core` and the runner
    /// example belong to the test section; the caller decides them once.
    fn lines(self, include_core: bool, runner: Option<ConfigTestRunner>) -> Vec<&'static str> {
        let profile = ConfigDocProfile::Schema;
        match self {
            Self::Manifest if include_core => docs::manifest_lines(profile),
            Self::Manifest => docs::manifest_minimal_lines(),
            // The manifest section's include graph overrides this entry from
            // `effigy.docs.toml`, and composition refuses an override path the
            // including manifest does not define. The template therefore
            // carries the index it shows being overridden.
            Self::DocsIndex => vec![
                "[docs_policy.indexes.topic]",
                "# Repository-owned docs index an agent reads first.",
                "file = \"docs/README.md\"",
                "dir = \"docs\"",
                "",
            ],
            Self::DocsPolicyGraph => docs::docs_policy_graph_lines(profile),
            Self::Containers => docs::containers_lines(profile),
            Self::PackageManager => docs::package_manager_lines(profile),
            Self::Test => docs::test_section_lines(
                include_core,
                profile,
                runner.map(ConfigTestRunner::as_str),
            ),
            Self::Secrets => docs::secrets_lines(profile),
            Self::State => docs::state_lines(profile),
            Self::Deploy => docs::deploy_lines(profile),
            Self::Defer => docs::defer_lines().to_vec(),
            Self::Scan => docs::scan_lines().to_vec(),
            Self::Tasks if include_core => docs::tasks_canonical_lines(profile),
            Self::Tasks => docs::tasks_minimal_lines().to_vec(),
        }
    }
}

/// The sections a whole template or one `--target` view documents.
fn sections_for(target: Option<ConfigSchemaTarget>, minimal: bool) -> &'static [SchemaSection] {
    match target {
        None if minimal => MINIMAL_SECTIONS,
        None => CANONICAL_SECTIONS,
        Some(ConfigSchemaTarget::Manifest) if minimal => &[SchemaSection::Manifest],
        Some(ConfigSchemaTarget::Manifest) => &[
            SchemaSection::Manifest,
            SchemaSection::DocsIndex,
            SchemaSection::DocsPolicyGraph,
        ],
        Some(ConfigSchemaTarget::PackageManager) => &[SchemaSection::PackageManager],
        Some(ConfigSchemaTarget::Test) => &[SchemaSection::Test],
        Some(ConfigSchemaTarget::Tasks) => &[SchemaSection::Tasks],
        Some(ConfigSchemaTarget::Defer) => &[SchemaSection::Defer],
        Some(ConfigSchemaTarget::Scan) => &[SchemaSection::Scan],
        Some(ConfigSchemaTarget::Bundle) => {
            unreachable!("bundle target is rendered dynamically")
        }
    }
}

fn render_schema_template(
    header: &str,
    sections: &[SchemaSection],
    minimal: bool,
    runner: Option<ConfigTestRunner>,
) -> String {
    let mut doc = TextDoc::new();
    doc.line(header);
    doc.blank();
    for section in sections {
        append_doc_lines(&mut doc, section.lines(!minimal, runner));
    }
    doc.finish()
}

pub(super) fn render_builtin_config_schema() -> String {
    render_schema_template(HEADER_CANONICAL, CANONICAL_SECTIONS, false, None)
}

pub(super) fn render_builtin_config_schema_minimal() -> String {
    render_schema_template(
        HEADER_MINIMAL,
        MINIMAL_SECTIONS,
        true,
        Some(ConfigTestRunner::Vitest),
    )
}

pub(super) fn render_builtin_config_schema_target(
    target: ConfigSchemaTarget,
    minimal: bool,
) -> String {
    if target == ConfigSchemaTarget::Bundle {
        return render_builtin_config_schema_bundle_target(minimal, None, &[]);
    }

    render_schema_template(
        &format!(
            "{} ({} target)",
            schema_header_prefix(minimal),
            target.as_str()
        ),
        sections_for(Some(target), minimal),
        minimal,
        None,
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

    render_schema_template(&header, &[SchemaSection::Test], minimal, runner)
}

fn schema_header_prefix(minimal: bool) -> &'static str {
    if minimal {
        HEADER_MINIMAL
    } else {
        HEADER_CANONICAL
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
            lines.push("# Use `effigy bundle sync` to update checkout source pins.".to_owned());
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

#[cfg(test)]
#[path = "schema/tests.rs"]
mod tests;
