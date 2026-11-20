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
        return Err(crate::retired_notice::format_error_detail(
            "`[bootstrap].run` is no longer supported",
            None,
            Some(
                "Replace it with `[bootstrap].setup = [\"<task>\"]` and move any inline commands into `[tasks.<name>]` (see CHANGELOG.md).",
            ),
            &[],
        ));
    }
    if let Some(setup) = bootstrap.get("setup") {
        match setup {
            Value::Array(entries) => {
                if let Some(table) = entries.iter().find(|entry| entry.is_table()) {
                    return Err(crate::retired_notice::format_error_detail(
                        "`[bootstrap].setup` entries must be task selector strings, not inline definitions",
                        Some(&format!("Found `{table}`.")),
                        Some(
                            "Declare the step as a `[tasks.<name>]` task and select it by name.",
                        ),
                        &[],
                    ));
                }
            }
            _ => {
                return Err(crate::retired_notice::format_error_detail(
                    "`[bootstrap].setup` takes a list of task selector strings",
                    None,
                    Some(
                        "Declare the inline definition as a `[tasks.<name>]` task and select that name.",
                    ),
                    &[],
                ))
            }
        }
    }
    if let Some(start) = bootstrap.get("start") {
        if !matches!(start, Value::String(_)) {
            return Err(crate::retired_notice::format_error_detail(
                "`[bootstrap].start` takes one task selector string",
                None,
                Some(
                    "Declare the chain or inline definition as a `[tasks.<name>]` task with `run = [{ task = \"...\" }, ...]` and select that name.",
                ),
                &[],
            ));
        }
    }
    if let Some(submodules) = bootstrap.get("submodules") {
        if !submodules.is_bool() {
            return Err(crate::retired_notice::format_error_detail(
                "`[bootstrap].submodules` takes a boolean",
                None,
                Some(
                    "Write `submodules = true` (or `false`) instead of the `\"none\"`/`\"init\"`/`\"recursive\"` policy strings.",
                ),
                &[],
            ));
        }
    }
    if bootstrap.contains_key("children") {
        return Err(crate::retired_notice::format_error_detail(
            "`[bootstrap].children` is no longer supported",
            None,
            Some(
                "Declare each sibling as a pinned `[sources.<name>]` table with `git`, `rev` (or `rev_file`) and `path`, and let the repo's `setup` selectors prepare it (see CHANGELOG.md).",
            ),
            &[],
        ));
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
        assert!(error.contains("`[bootstrap].run` is no longer supported"), "{error}");
        assert!(error.contains("[bootstrap].setup"), "{error}");
        assert!(error.contains("Next:"), "{error}");
        assert!(!error.contains("was accepted through"), "{error}");
    }

    #[test]
    fn an_inline_setup_or_start_definition_names_a_task() {
        let setup = error("[bootstrap]\nsetup = { rhai = \"scripts/bootstrap.rhai\" }\n");
        assert!(setup.contains("[bootstrap].setup"), "{setup}");
        assert!(setup.contains("[tasks.<name>]"), "{setup}");
        assert!(setup.contains("Next:"), "{setup}");
        let start = error("[bootstrap]\nstart = [{ task = \"dev\" }, { task = \"logs\" }]\n");
        assert!(start.contains("[bootstrap].start"), "{start}");
        assert!(start.contains("[tasks.<name>]"), "{start}");
        assert!(start.contains("Next:"), "{start}");
    }

    #[test]
    fn a_submodule_policy_string_names_the_boolean() {
        let error = error("[bootstrap]\nsubmodules = \"recursive\"\n");
        assert!(error.contains("submodules"), "{error}");
        assert!(error.contains("boolean"), "{error}");
        assert!(error.contains("Next:"), "{error}");
    }

    #[test]
    fn a_children_table_names_pinned_sources() {
        let error = error("[bootstrap]\nchildren = []\n");
        assert!(error.contains("`[bootstrap].children` is no longer supported"), "{error}");
        assert!(error.contains("[sources.<name>]"), "{error}");
        assert!(error.contains("Next:"), "{error}");
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
