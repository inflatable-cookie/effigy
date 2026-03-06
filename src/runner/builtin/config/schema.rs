use super::super::text_doc::TextDoc;
use super::docs::{self, ConfigDocProfile};
use super::request::{ConfigSchemaTarget, ConfigTestRunner};

const HEADER_CANONICAL: &str = "# Canonical strict-valid effigy.toml schema template";
const HEADER_MINIMAL: &str = "# Minimal strict-valid effigy.toml starter";

fn prefixed_section(header: &str, lines: impl IntoIterator<Item = &'static str>) -> String {
    let mut doc = TextDoc::new();
    doc.line(header);
    doc.blank();
    append_lines(&mut doc, lines);
    doc.finish()
}

pub(super) fn render_builtin_config_schema() -> String {
    let mut doc = TextDoc::new();
    doc.line(HEADER_CANONICAL);
    doc.blank();
    append_lines(
        &mut doc,
        docs::package_manager_lines(ConfigDocProfile::Schema),
    );
    append_lines(
        &mut doc,
        docs::test_section_lines(true, ConfigDocProfile::Schema, None),
    );
    append_lines(&mut doc, docs::defer_lines().iter().copied());
    append_lines(&mut doc, docs::shell_lines().iter().copied());
    append_lines(&mut doc, docs::scan_lines().iter().copied());
    append_lines(
        &mut doc,
        docs::tasks_canonical_lines(ConfigDocProfile::Schema),
    );
    doc.finish()
}

pub(super) fn render_builtin_config_schema_minimal() -> String {
    let mut doc = TextDoc::new();
    doc.line(HEADER_MINIMAL);
    doc.blank();
    append_lines(
        &mut doc,
        docs::package_manager_lines(ConfigDocProfile::Schema),
    );
    append_lines(
        &mut doc,
        docs::test_section_lines(false, ConfigDocProfile::Schema, Some("vitest")),
    );
    append_lines(&mut doc, docs::tasks_minimal_lines().iter().copied());
    doc.finish()
}

pub(super) fn render_builtin_config_schema_target(
    target: ConfigSchemaTarget,
    minimal: bool,
) -> String {
    let header_prefix = if minimal {
        "# Minimal strict-valid effigy.toml starter"
    } else {
        "# Canonical strict-valid effigy.toml schema template"
    };

    match (target, minimal) {
        (ConfigSchemaTarget::PackageManager, true)
        | (ConfigSchemaTarget::PackageManager, false) => prefixed_section(
            &format!("{header_prefix} (package_manager target)"),
            docs::package_manager_lines(ConfigDocProfile::Schema),
        ),
        (ConfigSchemaTarget::Tasks, true) => prefixed_section(
            &format!("{header_prefix} (tasks target)"),
            docs::tasks_minimal_lines().iter().copied(),
        ),
        (ConfigSchemaTarget::Tasks, false) => prefixed_section(
            &format!("{header_prefix} (tasks target)"),
            docs::tasks_canonical_lines(ConfigDocProfile::Schema),
        ),
        (ConfigSchemaTarget::Defer, true) | (ConfigSchemaTarget::Defer, false) => prefixed_section(
            &format!("{header_prefix} (defer target)"),
            docs::defer_lines().iter().copied(),
        ),
        (ConfigSchemaTarget::Scan, true) | (ConfigSchemaTarget::Scan, false) => prefixed_section(
            &format!("{header_prefix} (scan target)"),
            docs::scan_lines().iter().copied(),
        ),
        (ConfigSchemaTarget::Shell, true) | (ConfigSchemaTarget::Shell, false) => prefixed_section(
            &format!("{header_prefix} (shell target)"),
            docs::shell_lines().iter().copied(),
        ),
        (ConfigSchemaTarget::Test, true) | (ConfigSchemaTarget::Test, false) => prefixed_section(
            &format!("{header_prefix} (test target)"),
            docs::test_section_lines(!minimal, ConfigDocProfile::Schema, None),
        ),
    }
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
    append_lines(
        &mut doc,
        docs::test_section_lines(
            !minimal,
            ConfigDocProfile::Schema,
            runner.map(ConfigTestRunner::as_str),
        ),
    );
    doc.finish()
}

fn append_lines(doc: &mut TextDoc, lines: impl IntoIterator<Item = &'static str>) {
    for line in lines {
        doc.line(line);
    }
}
