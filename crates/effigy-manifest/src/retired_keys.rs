//! Retired task-manifest keys.
//!
//! A retired table is refused with the replacement an operator can act on,
//! never silently accepted or translated (contract 043, `retired.toml`).
use serde::Deserialize;
use toml::Value;

use crate::TaskManifest;

impl<'de> Deserialize<'de> for TaskManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        reject_retired_tables(&value).map_err(serde::de::Error::custom)?;
        TaskManifest::deserialize(value).map_err(serde::de::Error::custom)
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
    if root.contains_key("shell") {
        return Err(
            "`[shell]` was removed in v0.15; managed shell tabs use `exec ${SHELL:-/bin/zsh} -i` and there is no manifest override (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    reject_bootstrap_keys(root)?;
    if let Some(scan) = root.get("scan").and_then(Value::as_table) {
        reject_retired_scan_sections(scan)?;
    }
    if let Some(tasks) = root.get("tasks").and_then(Value::as_table) {
        if tasks.contains_key("bootstrap:db-seed") {
            return Err(
                "the `bootstrap:db-seed` task was retired in v0.15; rename it to `data:seed` (the callback `effigy container data seed` runs) and select it from `[bootstrap].setup` when a fresh checkout needs a first seed (see CHANGELOG.md)"
                    .to_owned(),
            );
        }
        for task in tasks.values() {
            reject_draft_in_task(task)?;
        }
    }
    Ok(())
}

/// `[bootstrap]` was rebuilt in v0.15 around two ordinary task selectors and
/// pinned sources. Each removed key names its replacement.
fn reject_bootstrap_keys(root: &toml::map::Map<String, Value>) -> Result<(), String> {
    let Some(bootstrap) = root.get("bootstrap").and_then(Value::as_table) else {
        return Ok(());
    };
    if bootstrap.contains_key("run") {
        return Err(
            "`[bootstrap].run` was renamed to `[bootstrap].setup` in v0.15 and now takes an ordered list of ordinary task selectors, for example `setup = [\"workspace:js:prepare\", \"db:seed:schema\"]`; move any inline run steps into a task and select it (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    if bootstrap.contains_key("children") {
        return Err(
            "`[bootstrap].children` was removed in v0.15; declare each sibling as a pinned `[sources.<name>]` table with `git`, `rev` (or `rev_file`) and `path`, and let the repo's `setup` selectors prepare it (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    if let Some(submodules) = bootstrap.get("submodules") {
        if !matches!(submodules, Value::Boolean(_)) {
            return Err(
                "`[bootstrap].submodules` now takes `true` (recursive init at the recorded commits) or is omitted; the `none`/`init`/`recursive` policy strings were removed in v0.15 (see CHANGELOG.md)"
                    .to_owned(),
            );
        }
    }
    if let Some(setup) = bootstrap.get("setup") {
        let Some(entries) = setup.as_array() else {
            return Err(
                "`[bootstrap].setup` is an ordered list of ordinary task selectors, for example `setup = [\"workspace:js:prepare\"]`; inline run steps were removed in v0.15, move them into a task (see CHANGELOG.md)"
                    .to_owned(),
            );
        };
        if entries
            .iter()
            .any(|entry| !matches!(entry, Value::String(_)))
        {
            return Err(
                "`[bootstrap].setup` entries must be task selector strings in v0.15; `{ task = ... }` and inline run steps were removed, move them into a task (see CHANGELOG.md)"
                    .to_owned(),
            );
        }
    }
    if let Some(start) = bootstrap.get("start") {
        if !matches!(start, Value::String(_)) {
            return Err(
                "`[bootstrap].start` now takes one task selector string in v0.15; arrays and `{ task = ... }` tables were removed, use a task that sequences the steps (see CHANGELOG.md)"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

/// Scan sections retired by the v0.15 scanner cut (Q-009) and by the
/// code-graph cut. Each is a manifest error rather than a silently ignored
/// table.
fn reject_retired_scan_sections(scan: &toml::value::Table) -> Result<(), String> {
    for (key, message) in [
        (
            "attention_markers",
            "`[scan.attention_markers]` was removed in v0.15; search markers with `rg -n 'TODO|FIXME' <path>` and delete the table (see CHANGELOG.md)",
        ),
        (
            "comment_ratio",
            "`[scan.comment_ratio]` was removed in v0.15 and has no replacement; delete the table (see CHANGELOG.md)",
        ),
        (
            "generated_assets",
            "`[scan.generated_assets]` was removed in v0.15 and has no replacement; keep bulky generated output out of versioned paths with `.gitignore` and delete the table (see CHANGELOG.md)",
        ),
        (
            "generated_in_src",
            "`[scan.generated_in_src]` was removed in v0.15 and has no replacement; keep generated output outside source trees and delete the table (see CHANGELOG.md)",
        ),
        (
            "stale_suppressions",
            "`[scan.stale_suppressions]` was removed in v0.15 and has no replacement; let each tool report its own unused suppressions and delete the table (see CHANGELOG.md)",
        ),
        (
            "boundary_violations",
            "`[scan.boundary_violations]` was removed in v0.15 with the code graph and has no replacement; delete the table and rely on code review (see CHANGELOG.md)",
        ),
        (
            "dead_code",
            "`[scan.dead_code]` was removed in v0.15 with the code graph and has no replacement; delete the table (see CHANGELOG.md)",
        ),
        (
            "validation_gaps",
            "`[scan.validation_gaps]` was removed in v0.15 with the code graph; delete the table and use `effigy test --plan` to choose tests for changed files (see CHANGELOG.md)",
        ),
    ] {
        if scan.contains_key(key) {
            return Err(message.to_owned());
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
    fn retirement_demos_table_is_refused_with_the_task_conversion_recipe() {
        let error = toml::from_str::<TaskManifest>(
            "[demos.checkout]\ntitle = 'Checkout'\nsummary = 'Pay for a cart'\nrun = 'bun run demo'",
        )
        .expect_err("retired table");
        let error = error.to_string();
        assert!(error.contains("`[demos]` was removed in v0.15"), "{error}");
        assert!(error.contains("`[tasks.\"demo:<name>\"]`"), "{error}");
        assert!(error.contains("`[demos.checkout]`"), "{error}");
        assert!(error.contains("`[tasks.\"demo:checkout\"]`"), "{error}");
        assert!(error.contains("group = \"Demos\""), "{error}");
        assert!(error.contains("CHANGELOG.md"), "{error}");
        assert!(!error.contains("external tool"), "{error}");
    }

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
    fn shell_table_is_refused_with_its_replacement() {
        let error = toml::from_str::<TaskManifest>("[shell]\nrun = 'exec zsh -i'")
            .expect_err("retired table");
        let error = error.to_string();
        assert!(error.contains("`[shell]` was removed in v0.15"), "{error}");
        assert!(
            error.contains("DEFAULT") || error.contains("/bin/zsh"),
            "{error}"
        );
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }

    #[test]
    fn retired_scan_sections_are_refused_with_their_replacement() {
        for (section, expected) in [
            ("comment_ratio", "no replacement"),
            ("generated_assets", ".gitignore"),
            ("generated_in_src", "generated output outside source trees"),
            ("attention_markers", "rg -n 'TODO|FIXME'"),
            ("stale_suppressions", "unused suppressions"),
        ] {
            let manifest = format!("[scan.{section}]\nwarn = 1");
            let error = toml::from_str::<TaskManifest>(&manifest).expect_err("retired table");
            let error = error.to_string();
            assert!(error.contains("removed in v0.15"), "{section}: {error}");
            assert!(error.contains(expected), "{section}: {error}");
            assert!(error.contains("CHANGELOG.md"), "{section}: {error}");
        }
    }

    #[test]
    fn bootstrap_run_is_refused_with_the_setup_key() {
        let error = toml::from_str::<TaskManifest>("[bootstrap]\nrun = [{ task = 'x' }]\n")
            .expect_err("retired bootstrap run");
        let error = error.to_string();
        assert!(error.contains("`[bootstrap].run` was renamed"), "{error}");
        assert!(error.contains("`[bootstrap].setup`"), "{error}");
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }

    #[test]
    fn bootstrap_children_are_refused_with_the_sources_key() {
        let error = toml::from_str::<TaskManifest>(
            "[bootstrap]\nstart = 'dev'\n\n[[bootstrap.children]]\npath = '../book'\nrepo = 'git@example.test:book.git'\n",
        )
        .expect_err("retired bootstrap children");
        let error = error.to_string();
        assert!(
            error.contains("`[bootstrap].children` was removed"),
            "{error}"
        );
        assert!(error.contains("[sources.<name>]"), "{error}");
        assert!(error.contains("rev_file"), "{error}");
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }

    #[test]
    fn bootstrap_submodule_policy_strings_are_refused() {
        let error = toml::from_str::<TaskManifest>("[bootstrap]\nsubmodules = 'recursive'\n")
            .expect_err("retired submodule policy");
        let error = error.to_string();
        assert!(
            error.contains("`[bootstrap].submodules` now takes `true`"),
            "{error}"
        );
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }

    #[test]
    fn bootstrap_inline_setup_steps_are_refused() {
        let error = toml::from_str::<TaskManifest>("[bootstrap]\nsetup = [{ task = 'x' }]\n")
            .expect_err("retired inline setup");
        let error = error.to_string();
        assert!(error.contains("selector strings"), "{error}");
        assert!(error.contains("CHANGELOG.md"), "{error}");
    }
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
}
