use super::super::surface::COMPLETION_COMMAND_OPTIONS;
use crate::constants::BUILTIN_TASKS;
use effigy_cli::command_surface::{
    group_for_child_word, namespace_children, namespace_words, HelpGroup,
};

/// Primary top-level completion candidates (spec `116`).
///
/// Grouped routes and their namespace descendants are primary; the daily
/// spine and remaining direct built-ins stay primary; displaced direct
/// spellings of grouped children are executable but no longer suggested.
/// Task selectors are merged by the candidates endpoint separately.
pub(super) fn command_names() -> Vec<&'static str> {
    let mut names = Vec::with_capacity(BUILTIN_TASKS.len() + 2);
    names.push("help");
    names.push("version");
    for (name, _) in BUILTIN_TASKS {
        if group_for_child_word(name).is_none() {
            names.push(name);
        }
    }
    names.extend(namespace_words());
    names
}

pub(super) fn command_rows() -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![
        ("help", "Show general help"),
        ("version", "Print the current Effigy version"),
    ];
    rows.extend(
        BUILTIN_TASKS
            .iter()
            .copied()
            .filter(|(name, _)| group_for_child_word(name).is_none()),
    );
    for group in HelpGroup::ALL {
        if *group != HelpGroup::Work {
            rows.push((group.slug(), group.summary()));
        }
    }
    rows
}

pub(super) fn command_options(command: &str) -> &'static [&'static str] {
    match command {
        "help" => &["--json", "--help", "-h"],
        "version" => &["--json", "--help", "-h"],
        "catalog" => &["cache", "clear", "--repo", "--json", "--help", "-h"],
        "tasks" => &[
            "--repo",
            "--task",
            "--resolve",
            "migrate",
            "unlock",
            "cache",
            "--json",
            "--pretty",
            "--help",
            "-h",
        ],
        "doctor" => &["--repo", "--fix", "--verbose", "--json", "--help", "-h"],
        "deps" => &[
            "status",
            "link",
            "unlink",
            "cargo",
            "bun",
            "--dry-run",
            "--repo",
            "--json",
            "--help",
            "-h",
        ],
        "test" => &[
            "--plan",
            "--verbose-results",
            "--tui",
            "--json",
            "--help",
            "-h",
        ],
        "watch" => &[
            "--owner",
            "--debounce-ms",
            "--include",
            "--exclude",
            "--once",
            "--max-runs",
            "--json",
            "--help",
            "-h",
        ],
        "init" => &[
            "--check",
            "--apply",
            "--repair",
            "--dry-run",
            "--force",
            "--json",
            "--help",
            "-h",
        ],
        "migrate" => &["--from", "--script", "--apply", "--json", "--help", "-h"],
        "config" => &[
            "path",
            "get",
            "set",
            "unset",
            "completion",
            "--schema",
            "--minimal",
            "--target",
            "--runner",
            "--json",
            "--help",
            "-h",
        ],
        "unlock" => &["--all", "--yes", "--json", "--help", "-h"],
        "cache" => &["inspect", "invalidate", "--all", "--json", "--help", "-h"],
        "completion" => COMPLETION_COMMAND_OPTIONS,
        // Grouped namespace descendants complete as word-2 candidates.
        word if HelpGroup::from_slug(word).is_some_and(|group| group != HelpGroup::Work) => {
            namespace_children(HelpGroup::from_slug(word).expect("checked"))
                .expect("executable namespace has children")
        }
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_primary_surface_excludes_displaced_direct_spellings() {
        // Displaced direct words leave the primary candidate list...
        for displaced in ["deps", "graph", "scan", "container", "release", "config"] {
            assert!(
                !command_names().contains(&displaced),
                "{displaced} must not be a primary completion candidate"
            );
        }
        // ...while the daily spine stays, and namespaces lead to descendants.
        for retained in [
            "help", "version", "tasks", "test", "watch", "doctor", "init",
        ] {
            assert!(command_names().contains(&retained), "{retained}");
        }
        for group in namespace_words() {
            assert!(command_names().contains(&group), "namespace {group}");
        }
        assert_eq!(
            command_options("repo"),
            ["graph", "scan", "docs", "contracts", "papercuts"]
        );
        assert_eq!(
            command_options("admin"),
            ["config", "deps", "secrets", "defer", "uninstall", "version"]
        );
        // Retained direct words keep their word-2 option arms.
        assert!(!command_options("tasks").is_empty());
        assert!(!command_options("config").is_empty());
    }
}
