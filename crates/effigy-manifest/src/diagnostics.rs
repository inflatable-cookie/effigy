//! Manifest shape diagnostics for readers that diagnose rather than load.
//!
//! `doctor` reports what a project can fix without running it. Loading
//! `effigy.toml` stops at the first refusal, which is right for a runner and
//! useless for a report, so this module asks the same parser one top-level
//! table at a time: each table is deserialized through the production grammar
//! ([`TaskManifest`], plus the composition-only `[manifest]` section), and a
//! failing table is narrowed to the members that actually break it. Keys are
//! therefore accepted here exactly when the loader accepts them, and every
//! message, including its list of accepted spellings, is the parser's own.

use toml::map::Map;
use toml::Value;

use crate::manifest_section::ManifestSectionConfig;
use crate::{TaskManifest, MANIFEST_TOP_LEVEL_KEYS};

/// One place the production grammar refuses, with the parser's own message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDiagnostic {
    /// Dotted key path, narrowed as far as the grammar allows: `tasks.probe`
    /// for an unknown task key, `test` when the section shape itself is wrong.
    pub path: String,
    /// The production parser's message for that value.
    pub message: String,
}

/// Every shape refusal in one parsed manifest document, by key path.
pub fn diagnose_manifest_value(value: &Value) -> Vec<ManifestDiagnostic> {
    let Some(table) = value.as_table() else {
        return vec![ManifestDiagnostic {
            path: String::new(),
            message: "the manifest root document must be a TOML table".to_owned(),
        }];
    };
    table
        .iter()
        .flat_map(|(key, entry)| diagnose(std::slice::from_ref(key), entry))
        .collect()
}

/// `keys` is where this value sits in the document; the loader accepts whole
/// tables, so a refusal is reproduced from the value and its own ancestors.
fn diagnose(keys: &[String], value: &Value) -> Vec<ManifestDiagnostic> {
    let Some(message) = refusal(keys, value) else {
        return Vec::new();
    };
    // Narrow a table the loader knows to the members that break it, so one bad
    // task or container does not hide its neighbours. A member that is itself a
    // table narrows further, down to the value the grammar refuses.
    if known_top_level_table(keys) {
        if let Some(members) = value.as_table() {
            let narrowed = members
                .iter()
                .flat_map(|(member, member_value)| {
                    let mut child = keys.to_vec();
                    child.push(member.clone());
                    diagnose(&child, member_value)
                })
                .collect::<Vec<_>>();
            if !narrowed.is_empty() {
                return narrowed;
            }
        }
    }
    vec![ManifestDiagnostic {
        path: keys.join("."),
        message: first_line(&message).to_owned(),
    }]
}

/// serde appends the section it was in as trailing context; the path already
/// says where the value is, so one line keeps the report readable.
fn first_line(message: &str) -> &str {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(message)
}

fn known_top_level_table(keys: &[String]) -> bool {
    keys.first()
        .is_some_and(|key| MANIFEST_TOP_LEVEL_KEYS.contains(&key.as_str()))
}

/// The production grammar's refusal for one value, if it refuses it.
fn refusal(keys: &[String], value: &Value) -> Option<String> {
    if keys.len() == 1 && keys[0] == "manifest" {
        return value
            .clone()
            .try_into::<ManifestSectionConfig>()
            .err()
            .map(|error| format!("invalid `[manifest]` section: {error}"));
    }
    let mut nested = value.clone();
    for key in keys.iter().skip(1).rev() {
        let mut parent = Map::new();
        parent.insert(key.clone(), nested);
        nested = Value::Table(parent);
    }
    let mut document = Map::new();
    document.insert(keys[0].clone(), nested);
    Value::Table(document)
        .try_into::<TaskManifest>()
        .err()
        .map(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::diagnose_manifest_value;

    fn diagnose(text: &str) -> Vec<(String, String)> {
        let value: toml::Value = toml::from_str(text).expect("fixture parses as TOML");
        diagnose_manifest_value(&value)
            .into_iter()
            .map(|diagnostic| (diagnostic.path, diagnostic.message))
            .collect()
    }

    #[test]
    fn a_manifest_the_loader_accepts_has_no_diagnostics() {
        assert!(diagnose(
            r#"
[manifest]
include = [{ path = "effigy.tasks.toml" }]

[catalog]
alias = "app"

[tasks.dev]
run = "cargo run"
description = "Run the dev stack"

[tasks]
qa = "cargo test"
"#
        )
        .is_empty());
    }

    /// The retired one-release tables still load, so diagnosing must not flag
    /// them; the manifest's own deprecation notice owns that statement.
    #[test]
    fn retired_tables_are_not_diagnostics() {
        assert!(
            diagnose("[shell]\nrun = \"exec zsh -i\"\n\n[scan.attention_markers]\nwarn = 1\n")
                .is_empty()
        );
    }

    /// An unsupported task key reports the production message, which names the
    /// accepted spellings, at the narrowest path that still refuses.
    #[test]
    fn an_unsupported_task_key_is_narrowed_to_its_task() {
        let diagnostics = diagnose(
            "[tasks.dev]\nrun = \"cargo run\"\n\n[tasks.one]\nrun = \"cargo run\"\ndurable = \"always\"\n",
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].0, "tasks.one.durable", "{diagnostics:?}");
        assert!(
            diagnostics[0].1.contains("durable must be a boolean"),
            "{diagnostics:?}"
        );
    }

    /// One bad member does not hide its siblings.
    #[test]
    fn every_broken_member_is_reported() {
        let diagnostics = diagnose(
            "[tasks.one]\nrun = \"cargo run\"\nscratch = \"never\"\n\n[tasks.two]\nrun = \"cargo test\"\ndurable = \"always\"\n",
        );
        let paths: Vec<&str> = diagnostics.iter().map(|(path, _)| path.as_str()).collect();
        assert_eq!(
            paths,
            ["tasks.one.scratch", "tasks.two.durable"],
            "{diagnostics:?}"
        );
    }

    #[test]
    fn an_unsupported_top_level_table_lists_the_accepted_tables() {
        let diagnostics = diagnose("[builtin.test]\nmax_parallel = 2\n");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].0, "builtin", "{diagnostics:?}");
        assert!(
            diagnostics[0].1.contains("unknown field `builtin`")
                && diagnostics[0].1.contains("`catalog`"),
            "{diagnostics:?}"
        );
    }

    /// A table the loader refuses outright keeps the loader's replacement
    /// advice rather than becoming an unknown-key report.
    #[test]
    fn a_refused_table_keeps_its_replacement_advice() {
        let diagnostics = diagnose("[drafts.proof]\nrun = \"echo old\"\n");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(
            diagnostics[0].1.contains("removed in v0.15"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn a_wrong_value_type_names_the_subtree() {
        let diagnostics = diagnose("[containers.web.dns]\nroutes = [{ port = \"web\" }]\n");
        assert_eq!(
            diagnostics[0].0, "containers.web.dns.routes",
            "{diagnostics:?}"
        );
        assert!(diagnostics[0].1.contains("invalid type"), "{diagnostics:?}");
    }
}
