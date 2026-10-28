//! What one manifest keeps loading from retired v0.15 config.
//!
//! A v0.14.1 project must keep loading after an update (contract 052 §11,
//! `docs/knowledge/vision.md`). Two retirements keep their old spelling for
//! the whole v0.15 line:
//!
//! - [`crate::retired_bootstrap`] translates retired `[bootstrap]` spellings;
//! - [`crate::retired_sections`] ignores a section that configured a feature
//!   v0.15 removed, when ignoring it changes nothing else.
//!
//! Both record what they read in one [`RetiredConfigDeclaration`] per
//! manifest. [`crate::retired_notice`] delivers its notice on the invocation's
//! channel; the declaration also lists the migration messages a removed
//! command still refuses with, and neither ever reaches `--json` stdout. The
//! compatibility path is removed after v0.15.

use std::path::Path;

/// What one manifest's retired v0.15 config read, ignored or refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetiredConfigDeclaration {
    statements: Vec<String>,
    refusals: Vec<String>,
    /// v0.15 spellings that still load and will be removed in v0.16.
    compat_statements: Vec<String>,
}

impl RetiredConfigDeclaration {
    pub fn is_empty(&self) -> bool {
        self.statements.is_empty() && self.refusals.is_empty() && self.compat_statements.is_empty()
    }

    /// Human-readable statements of what Effigy read or ignored in place of a
    /// retired declaration.
    pub fn statements(&self) -> &[String] {
        &self.statements
    }

    /// v0.15 spellings that still parse and will be removed in v0.16.
    pub fn compat_statements(&self) -> &[String] {
        &self.compat_statements
    }

    /// The actionable migration messages a removed command refuses with.
    pub fn refusals(&self) -> &[String] {
        &self.refusals
    }

    /// One concise deprecation notice for this manifest, or `None`.
    pub fn notice(&self, manifest_path: &Path) -> Option<String> {
        match (
            self.statements.is_empty(),
            self.compat_statements.is_empty(),
        ) {
            (true, true) => None,
            (false, true) => Some(format!(
                "{}: retired v0.15 config still loads for one release: {}. Update the manifest; this compatibility path is removed after v0.15 (see CHANGELOG.md and docs/guides/085-v0.15.0-consumer-migration.md).",
                manifest_path.display(),
                self.statements.join("; ")
            )),
            (true, false) => Some(format!(
                "{}: these v0.15 spellings remain accepted and will be removed in v0.16: {}. See CHANGELOG.md and docs/guides/070-per-machine-overlays-and-external-mounts.md.",
                manifest_path.display(),
                self.compat_statements.join("; ")
            )),
            (false, false) => Some(format!(
                "{}: retired v0.15 config still loads for one release: {}. Update the manifest; this compatibility path is removed after v0.15. Also, these v0.15 spellings remain accepted and will be removed in v0.16: {}. See CHANGELOG.md, docs/guides/085-v0.15.0-consumer-migration.md and docs/guides/070-per-machine-overlays-and-external-mounts.md.",
                manifest_path.display(),
                self.statements.join("; "),
                self.compat_statements.join("; ")
            )),
        }
    }

    /// The one refusal a removed command reports for this manifest, or `None`.
    pub fn refusal_message(&self) -> Option<String> {
        if self.refusals.is_empty() {
            return None;
        }
        Some(format!(
            "`effigy bootstrap` refuses this manifest until its retired bootstrap keys are migrated:\n- {}",
            self.refusals.join("\n- ")
        ))
    }

    pub(crate) fn record(&mut self, statement: impl Into<String>) {
        let statement = statement.into();
        if !self.statements.contains(&statement) {
            self.statements.push(statement);
        }
    }

    pub(crate) fn record_compat(&mut self, statement: impl Into<String>) {
        let statement = statement.into();
        if !self.compat_statements.contains(&statement) {
            self.compat_statements.push(statement);
        }
    }

    pub(crate) fn refuse(&mut self, message: &str) {
        if !self.refusals.iter().any(|refusal| refusal == message) {
            self.refusals.push(message.to_owned());
        }
    }

    pub(crate) fn merge(&mut self, other: Self) {
        for statement in other.statements {
            self.record(statement);
        }
        for statement in other.compat_statements {
            self.record_compat(statement);
        }
        for refusal in other.refusals {
            self.refuse(&refusal);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::RetiredConfigDeclaration;

    #[test]
    fn a_declaration_without_statements_has_no_notice() {
        let mut declaration = RetiredConfigDeclaration::default();
        assert!(declaration.notice(Path::new("effigy.toml")).is_none());
        declaration.refuse("`[bootstrap].children` was removed in v0.15");
        assert!(declaration.notice(Path::new("effigy.toml")).is_none());
        assert!(declaration.refusal_message().is_some());
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

    #[test]
    fn compat_statements_print_one_v016_notice() {
        let mut declaration = RetiredConfigDeclaration::default();
        declaration.record_compat("`schema-index` is accepted through v0.15; write `schema_index` instead. This spelling is removed in v0.16");
        declaration.record_compat("`[manifest].include` `optional` is accepted through v0.15; omit a missing include, or rely on auto-discovered `effigy.local.toml`. This form is removed in v0.16");
        let notice = declaration
            .notice(Path::new("/repo/effigy.toml"))
            .expect("notice");
        assert!(
            notice.starts_with("/repo/effigy.toml: these v0.15 spellings remain accepted"),
            "{notice}"
        );
        assert!(notice.contains("schema-index"), "{notice}");
        assert!(notice.contains("optional"), "{notice}");
        assert!(notice.contains("removed in v0.16"), "{notice}");
        assert_eq!(notice.matches("effigy.toml").count(), 1, "{notice}");
        assert!(
            notice.contains("docs/guides/070-per-machine-overlays-and-external-mounts.md"),
            "{notice}"
        );
        assert!(
            !notice.contains("retired v0.15 config still loads"),
            "{notice}"
        );
    }

    #[test]
    fn mixed_retired_and_compat_statements_stay_one_notice() {
        let mut declaration = RetiredConfigDeclaration::default();
        declaration.record("`[shell]` is ignored");
        declaration.record_compat("`rev-file` is accepted through v0.15; write `rev_file` instead. This spelling is removed in v0.16");
        let notice = declaration
            .notice(Path::new("/repo/effigy.toml"))
            .expect("notice");
        assert!(notice.contains("[shell]"), "{notice}");
        assert!(notice.contains("rev-file"), "{notice}");
        assert!(notice.contains("removed after v0.15"), "{notice}");
        assert!(notice.contains("removed in v0.16"), "{notice}");
        assert_eq!(notice.matches("effigy.toml").count(), 1, "{notice}");
    }
}
