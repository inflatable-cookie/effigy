//! v0.15 spellings that still parse and will be removed in v0.16.
//!
//! Manual `[manifest].include` `override` / `optional` and the public kebab-case
//! aliases still load with the same effective values. One compat statement per
//! form is recorded on the manifest's [`RetiredConfigDeclaration`] so the
//! invocation prints a single notice naming the canonical replacement.
//!
//! Inventory of public kebab serde aliases (position → snake):
//! - `[contracts].schema-index` → `schema_index`
//! - `[contracts].selection-contract` → `selection_contract`
//! - `[sources.<name>].rev-file` → `rev_file`
//!
//! `docs_policy.graph.kinds.<kind>.default-currentness` is the contract 041
//! spelling (`rename_all = "kebab-case"`). It is not in this window.
//!
//! Out of scope: user-global program preference aliases (`colima-nerdctl`,
//! `docker-compose`) and snake_case aliases such as `threshold` / `js_ts`.
//!
//! 2026-10-28 scan: `default-currentness` in Effigy's docs config and Northstar
//! starter (canonical); no active external consumer `effigy.toml` hit in
//! projects/legacy.

use toml::Value;

use crate::retired_config::RetiredConfigDeclaration;

pub(crate) const OPTIONAL_STATEMENT: &str = "`[manifest].include` `optional` is accepted through v0.15; omit a missing include, or rely on auto-discovered `effigy.local.toml`. This form is removed in v0.16";

pub(crate) const OVERRIDE_STATEMENT: &str = "`[manifest].include` `override` is accepted through v0.15; use auto-discovered `effigy.local.toml` for machine-local replacement, or `[manifest].extend` for arrays. This form is removed in v0.16";

pub(crate) fn scan(value: &Value) -> RetiredConfigDeclaration {
    let mut declaration = RetiredConfigDeclaration::default();
    scan_include_directives(value, &mut declaration);
    scan_aliases(value, &mut declaration);
    declaration
}

fn scan_include_directives(value: &Value, declaration: &mut RetiredConfigDeclaration) {
    let Some(entries) = value
        .get("manifest")
        .and_then(|section| section.get("include"))
        .and_then(Value::as_array)
    else {
        return;
    };
    for entry in entries {
        let Some(table) = entry.as_table() else {
            continue;
        };
        if table.contains_key("optional") {
            declaration.record_compat(OPTIONAL_STATEMENT);
        }
        if table.contains_key("override") {
            declaration.record_compat(OVERRIDE_STATEMENT);
        }
    }
}

fn scan_aliases(value: &Value, declaration: &mut RetiredConfigDeclaration) {
    if let Some(table) = value.get("contracts").and_then(Value::as_table) {
        notice_key(table, "schema-index", "schema_index", declaration);
        notice_key(
            table,
            "selection-contract",
            "selection_contract",
            declaration,
        );
    }
    if let Some(sources) = value.get("sources").and_then(Value::as_table) {
        for source in sources.values() {
            if let Some(table) = source.as_table() {
                notice_key(table, "rev-file", "rev_file", declaration);
            }
        }
    }
}

fn notice_key(
    table: &toml::Table,
    legacy: &str,
    canonical: &str,
    declaration: &mut RetiredConfigDeclaration,
) {
    if table.contains_key(legacy) {
        declaration.record_compat(alias_statement(legacy, canonical));
    }
}

fn alias_statement(legacy: &str, canonical: &str) -> String {
    format!(
        "`{legacy}` is accepted through v0.15; write `{canonical}` instead. This spelling is removed in v0.16"
    )
}

#[cfg(test)]
mod tests {
    use super::{scan, OPTIONAL_STATEMENT, OVERRIDE_STATEMENT};

    #[test]
    fn path_only_includes_and_snake_case_keys_are_silent() {
        let value: toml::Value = toml::from_str(
            r#"
[manifest]
include = ["tasks.toml"]

[contracts]
schema_index = "index.json"
selection_contract = "selection.json"
"#,
        )
        .expect("parse");
        assert!(scan(&value).is_empty());
    }

    #[test]
    fn include_optional_and_override_each_record_one_compat_statement() {
        let value: toml::Value = toml::from_str(
            r#"
[manifest]
include = [
  { path = "missing.toml", optional = true },
  { path = "overlay.toml", override = ["tasks.dev"] },
]
"#,
        )
        .expect("parse");
        let declaration = scan(&value);
        assert_eq!(
            declaration.compat_statements(),
            [OPTIONAL_STATEMENT, OVERRIDE_STATEMENT]
        );
    }

    #[test]
    fn representative_kebab_aliases_record_one_statement_each() {
        let value: toml::Value = toml::from_str(
            r#"
[contracts]
schema-index = "index.json"
selection-contract = "selection.json"

[sources.book]
git = "git@example.com:acme/book.git"
rev-file = "book.rev"
path = "../book"

[docs_policy.graph.kinds.guide]
include = ["docs/guides/*.md"]
default-currentness = "current"
"#,
        )
        .expect("parse");
        let declaration = scan(&value);
        let statements = declaration.compat_statements();
        assert_eq!(statements.len(), 3, "{statements:?}");
        assert!(statements
            .iter()
            .any(|item| item.contains("`schema-index`")));
        assert!(statements
            .iter()
            .any(|item| item.contains("`selection-contract`")));
        assert!(statements.iter().any(|item| item.contains("`rev-file`")));
        assert!(statements
            .iter()
            .all(|item| !item.contains("`default-currentness`")));
        assert!(statements
            .iter()
            .all(|item| item.contains("removed in v0.16")));
    }

    #[test]
    fn a_combined_legacy_document_does_not_repeat_the_same_statement() {
        let value: toml::Value = toml::from_str(
            r#"
[manifest]
include = [
  { path = "a.toml", optional = true, override = ["tasks.dev"] },
  { path = "b.toml", optional = true },
]

[contracts]
schema-index = "one.json"
"#,
        )
        .expect("parse");
        let declaration = scan(&value);
        let statements = declaration.compat_statements();
        assert_eq!(statements.len(), 3, "{statements:?}");
        assert_eq!(
            statements
                .iter()
                .filter(|item| item.as_str() == OPTIONAL_STATEMENT)
                .count(),
            1
        );
        assert_eq!(
            statements
                .iter()
                .filter(|item| item.contains("`schema-index`"))
                .count(),
            1
        );
    }

    #[test]
    fn a_task_named_schema_index_is_silent() {
        let value: toml::Value = toml::from_str(
            r#"
[tasks.schema-index]
run = "echo hi"
"#,
        )
        .expect("parse");
        assert!(scan(&value).is_empty());
    }

    #[test]
    fn nested_unrelated_schema_index_keys_are_silent() {
        let value: toml::Value = toml::from_str(
            r#"
[other.contracts]
schema-index = "two.json"

[sources.book]
git = "git@example.com:acme/book.git"
rev = "abc"
path = "book"
schema-index = "not-an-alias.json"
"#,
        )
        .expect("parse");
        assert!(scan(&value).is_empty());
    }

    #[test]
    fn docs_policy_kind_default_currentness_is_canonical_and_silent() {
        let value: toml::Value = toml::from_str(
            r#"
[docs_policy.graph.kinds.guide]
include = ["docs/guides/*.md"]
default-currentness = "current"
"#,
        )
        .expect("parse");
        assert!(scan(&value).is_empty());
    }
}
