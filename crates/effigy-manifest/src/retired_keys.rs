//! Retired task-manifest keys.
//!
//! A v0.14.1 project keeps loading after an update (contract 052 §11), so two
//! retirements are read for the whole v0.15 line instead of failing the parse:
//! [`crate::retired_bootstrap`] translates retired `[bootstrap]` spellings, and
//! [`crate::retired_sections`] ignores sections that configured a feature
//! v0.15 removed when ignoring them changes nothing else. Both record one
//! deprecation notice per manifest ([`crate::retired_config`]).
//!
//! `[drafts]` and `draft` step references are still refused, with the
//! replacement an operator can act on (contract 043, `retired.toml`):
//! ignoring them would silently drop task definitions.
use serde::Deserialize;
use toml::Value;

use crate::TaskManifest;

impl<'de> Deserialize<'de> for TaskManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = Value::deserialize(deserializer)?;
        let mut retired = crate::retired_sections::strip(&mut value);
        reject_retired_tables(&value).map_err(serde::de::Error::custom)?;
        retired.merge(crate::retired_bootstrap::translate(&mut value));
        let mut manifest = TaskManifest::deserialize(value).map_err(serde::de::Error::custom)?;
        manifest.retired_config = retired;
        Ok(manifest)
    }
}

fn reject_retired_tables(value: &Value) -> Result<(), String> {
    let Some(root) = value.as_table() else {
        return Ok(());
    };
    if root.contains_key("drafts") {
        return Err(
            "`[drafts]` was removed in v0.15; move each definition to `[tasks.<name>]` with `visibility = \"internal\"`, remove `created`/`expires`/`purpose`, and use ordinary `task` step references (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    if let Some(tasks) = root.get("tasks").and_then(Value::as_table) {
        for task in tasks.values() {
            reject_draft_in_task(task)?;
        }
    }
    Ok(())
}

/// A `draft = "<task>"` step reference resolved through the removed draft
/// namespace. Only `task` remains.
fn reject_draft_in_step(step: &Value) -> Result<(), String> {
    if step
        .as_table()
        .is_some_and(|table| table.contains_key("draft"))
    {
        return Err(
            "`draft` step reference was removed in v0.15; use `task = \"<name>\"` and declare the task under `[tasks]` with `visibility = \"internal\"` when it is not public (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    Ok(())
}

fn reject_draft_in_task(value: &Value) -> Result<(), String> {
    match value {
        Value::Array(steps) => {
            for step in steps {
                reject_draft_in_step(step)?;
            }
        }
        Value::Table(body) => {
            if let Some(run) = body.get("run") {
                match run {
                    Value::Array(steps) => {
                        for step in steps {
                            reject_draft_in_step(step)?;
                        }
                    }
                    Value::Table(_) => reject_draft_in_step(run)?,
                    _ => {}
                }
            }
            if let Some(concurrent) = body.get("concurrent").and_then(Value::as_array) {
                for entry in concurrent {
                    if let Some(setup) = entry.get("setup").and_then(Value::as_array) {
                        for step in setup {
                            reject_draft_in_step(step)?;
                        }
                    }
                }
            }
            if let Some(profiles) = body.get("profiles").and_then(Value::as_table) {
                for profile in profiles.values() {
                    reject_draft_in_task(profile)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::TaskManifest;

    #[test]
    fn drafts_table_is_refused_with_the_task_namespace_replacement() {
        let error = toml::from_str::<TaskManifest>(
            "[tasks]\nproof = 'echo proof'\n[drafts.proof]\nrun = 'echo draft'",
        )
        .expect_err("retired table");
        let error = error.to_string();
        assert!(error.contains("`[drafts]` was removed in v0.15"), "{error}");
        assert!(error.contains("[tasks.<name>]"), "{error}");
        assert!(error.contains("visibility = \"internal\""), "{error}");
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }

    #[test]
    fn draft_step_reference_is_refused_with_the_task_key() {
        let error = toml::from_str::<TaskManifest>(
            "[tasks]\nentry = [{ draft = 'helper' }]\nhelper = 'echo helper'",
        )
        .expect_err("retired step reference");
        let error = error.to_string();
        assert!(
            error.contains("`draft` step reference was removed"),
            "{error}"
        );
        assert!(error.contains("task ="), "{error}");
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }

    #[test]
    fn a_manifest_with_ignored_sections_and_drafts_still_refuses_drafts() {
        let error = toml::from_str::<TaskManifest>(
            "[scan.attention_markers]\nwarn = 1\n\n[drafts.proof]\nrun = 'echo draft'",
        )
        .expect_err("drafts stay a manifest error");
        assert!(error.to_string().contains("`[drafts]`"), "{error}");
    }
}
