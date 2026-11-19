//! Refuse `[bootstrap]` spellings retired in v0.15.
//!
//! Q-009 rebuilt `effigy bootstrap` around ordinary task selectors
//! (`setup`/`start`) and pinned `[sources.<name>]`. v0.15 accepted the old
//! spellings for one release; v0.16 refuses each with the replacement, so a
//! project that still carries one learns exactly what to write instead.
//!
//! - `run` → `setup`, whose entries are task selector strings.
//! - inline `run`/`rhai`/command steps and `{ task = ... }` entries →
//!   `[tasks.<name>]` definitions selected from `setup`.
//! - `start` chains and inline `start` definitions → one task selector.
//! - `submodules = "none"|"init"|"recursive"` → a boolean.
//! - `[bootstrap].children` → pinned `[sources.<name>]` tables.

use toml::Value;

/// Refuse every retired `[bootstrap]` spelling in `root`.
pub(crate) fn reject(root: &Value) -> Result<(), String> {
    let Some(bootstrap) = root.get("bootstrap").and_then(Value::as_table) else {
        return Ok(());
    };
    if bootstrap.contains_key("run") {
        return Err(
            "`[bootstrap].run` was accepted through v0.15 only; list task selectors in `[bootstrap].setup = [\"<selector>\"]` and declare any inline step as a `[tasks.<name>]` task it selects (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    if let Some(setup) = bootstrap.get("setup") {
        match setup {
            Value::Array(entries) => {
                if let Some(table) = entries.iter().find(|entry| entry.is_table()) {
                    return Err(format!(
                        "`[bootstrap].setup` takes task selector strings; `{table}` was accepted through v0.15 only. Declare the step as a `[tasks.<name>]` task and select it by name"
                    ));
                }
            }
            _ => {
                return Err(
                    "`[bootstrap].setup` takes a list of task selector strings; an inline definition was accepted through v0.15 only. Declare it as a `[tasks.<name>]` task and select that name"
                        .to_owned(),
                )
            }
        }
    }
    if let Some(start) = bootstrap.get("start") {
        if !matches!(start, Value::String(_)) {
            return Err(
                "`[bootstrap].start` takes one task selector string; a chain or inline definition was accepted through v0.15 only. Declare the sequence as a `[tasks.<name>]` task with `run = [{ task = \"...\" }, ...]` and select that name"
                    .to_owned(),
            );
        }
    }
    if let Some(submodules) = bootstrap.get("submodules") {
        if !submodules.is_bool() {
            return Err(
                "`[bootstrap].submodules` takes a boolean; the `\"none\"`/`\"init\"`/`\"recursive\"` policy strings were accepted through v0.15 only"
                    .to_owned(),
            );
        }
    }
    if bootstrap.contains_key("children") {
        return Err(
            "`[bootstrap].children` was accepted through v0.15 only; declare each sibling as a pinned `[sources.<name>]` table with `git`, `rev` (or `rev_file`) and `path`, and let the repo's `setup` selectors prepare it (see CHANGELOG.md)"
                .to_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::TaskManifest;

    fn error(text: &str) -> String {
        toml::from_str::<TaskManifest>(text)
            .expect_err("retired bootstrap spellings are refused")
            .to_string()
    }

    #[test]
    fn a_retired_run_list_names_setup() {
        let error = error("[bootstrap]\nrun = \"echo hi\"\n");
        assert!(error.contains("`[bootstrap].run`"), "{error}");
        assert!(error.contains("[bootstrap].setup"), "{error}");
    }

    #[test]
    fn an_inline_setup_or_start_definition_names_a_task() {
        let setup = error("[bootstrap]\nsetup = { rhai = \"scripts/bootstrap.rhai\" }\n");
        assert!(setup.contains("[bootstrap].setup"), "{setup}");
        assert!(setup.contains("[tasks.<name>]"), "{setup}");
        let start = error("[bootstrap]\nstart = [{ task = \"dev\" }, { task = \"logs\" }]\n");
        assert!(start.contains("[bootstrap].start"), "{start}");
        assert!(start.contains("[tasks.<name>]"), "{start}");
    }

    #[test]
    fn a_submodule_policy_string_names_the_boolean() {
        let error = error("[bootstrap]\nsubmodules = \"recursive\"\n");
        assert!(error.contains("submodules"), "{error}");
        assert!(error.contains("boolean"), "{error}");
    }

    #[test]
    fn a_children_table_names_pinned_sources() {
        let error = error("[bootstrap]\nchildren = []\n");
        assert!(error.contains("children"), "{error}");
        assert!(error.contains("[sources.<name>]"), "{error}");
    }

    #[test]
    fn the_current_bootstrap_shape_parses() {
        let manifest: TaskManifest = toml::from_str(
            "[bootstrap]\nsetup = [\"prepare\"]\nstart = \"dev\"\nsubmodules = true\n\n[tasks.prepare]\nrun = \"true\"\n\n[tasks.dev]\nrun = \"true\"\n",
        )
        .expect("current bootstrap shape");
        let bootstrap = manifest.bootstrap.expect("bootstrap");
        assert_eq!(bootstrap.setup, vec!["prepare"]);
        assert_eq!(bootstrap.start.as_deref(), Some("dev"));
        assert!(bootstrap.submodules);
    }
}
