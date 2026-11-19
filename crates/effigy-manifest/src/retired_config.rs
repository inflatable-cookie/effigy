//! What one manifest keeps loading from retired v0.15 config.
//!
//! A v0.14.1 project must keep loading after an update (contract 052 §11,
//! `docs/knowledge/vision.md`). One retirement keeps its old spelling for the
//! whole v0.15 line: [`crate::retired_sections`] ignores a section that
//! configured a feature v0.15 removed, when ignoring it changes nothing else.
//!
//! It records what it read in one [`RetiredConfigDeclaration`] per manifest.
//! [`crate::retired_notice`] delivers its notice on the invocation's channel;
//! the declaration never reaches `--json` stdout. The compatibility path is
//! removed after v0.15.

use std::path::Path;

/// What one manifest's retired v0.15 config read or ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetiredConfigDeclaration {
    statements: Vec<String>,
}

impl RetiredConfigDeclaration {
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty()
    }

    /// Human-readable statements of what Effigy read or ignored in place of a
    /// retired declaration.
    pub fn statements(&self) -> &[String] {
        &self.statements
    }

    /// One concise deprecation notice for this manifest, or `None`.
    pub fn notice(&self, manifest_path: &Path) -> Option<String> {
        if self.statements.is_empty() {
            return None;
        }
        Some(format!(
            "{}: retired v0.15 config still loads for one release: {}. Update the manifest; this compatibility path is removed after v0.15 (see CHANGELOG.md and docs/guides/085-v0.15.0-consumer-migration.md).",
            manifest_path.display(),
            self.statements.join("; ")
        ))
    }

    pub(crate) fn record(&mut self, statement: impl Into<String>) {
        let statement = statement.into();
        if !self.statements.contains(&statement) {
            self.statements.push(statement);
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        for statement in other.statements {
            self.record(statement);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::RetiredConfigDeclaration;

    #[test]
    fn a_declaration_without_statements_has_no_notice() {
        let declaration = RetiredConfigDeclaration::default();
        assert!(declaration.notice(Path::new("effigy.toml")).is_none());
    }

    #[test]
    fn one_declaration_prints_one_notice_naming_the_manifest() {
        let mut declaration = RetiredConfigDeclaration::default();
        declaration.record("`[scan.attention_markers]` is ignored");
        declaration.record("`[shell]` is ignored");
        let notice = declaration
            .notice(Path::new("/repo/effigy.toml"))
            .expect("notice");
        assert!(
            notice.starts_with("/repo/effigy.toml: retired v0.15 config"),
            "{notice}"
        );
        assert!(notice.contains("[scan.attention_markers]"), "{notice}");
        assert!(notice.contains("[shell]"), "{notice}");
        assert_eq!(notice.matches("effigy.toml").count(), 1, "{notice}");
        assert!(
            notice.contains("docs/guides/085-v0.15.0-consumer-migration.md"),
            "{notice}"
        );
    }

    #[test]
    fn merged_declarations_keep_one_statement_per_section() {
        let mut first = RetiredConfigDeclaration::default();
        first.record("`[shell]` is ignored");
        let mut second = RetiredConfigDeclaration::default();
        second.record("`[shell]` is ignored");
        second.record("`[demos]` is ignored");
        first.merge(second);
        assert_eq!(
            first.statements(),
            ["`[shell]` is ignored", "`[demos]` is ignored"]
        );
    }
}
