use super::super::doc_render::{append_doc_lines, render_prefixed_doc};
use super::super::text_doc::TextDoc;
use super::docs::{self, ConfigDocProfile};
use super::request::{ConfigSchemaTarget, ConfigTestRunner};

const HEADER_CANONICAL: &str = "# Canonical strict-valid effigy.toml schema template";
const HEADER_MINIMAL: &str = "# Minimal strict-valid effigy.toml starter";

pub(super) fn render_builtin_config_schema() -> String {
    let mut doc = TextDoc::new();
    doc.line(HEADER_CANONICAL);
    doc.blank();
    append_doc_lines(&mut doc, docs::manifest_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::distribution_lines(ConfigDocProfile::Schema));
    append_doc_lines(&mut doc, docs::demos_lines(ConfigDocProfile::Schema));
    append_doc_lines(
        &mut doc,
        docs::package_manager_lines(ConfigDocProfile::Schema),
    );
    append_doc_lines(
        &mut doc,
        docs::test_section_lines(true, ConfigDocProfile::Schema, None),
    );
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
    render_prefixed_doc(
        &format!(
            "{} ({} target)",
            schema_header_prefix(minimal),
            target.as_str()
        ),
        target_schema_lines(target, minimal),
    )
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
