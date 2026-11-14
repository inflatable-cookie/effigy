use effigy_cli::builtin_names::live_builtin_command_names;

use crate::constants::BUILTIN_TASKS;

/// One-line completion menu description for a live direct command that is not
/// a built-in task row.
///
/// The *set* of commands is owned by
/// `effigy_cli::builtin_names::live_builtin_command_names`; this table only
/// describes the words that have no built-in task description to borrow.
const DIRECT_COMMAND_DESCRIPTIONS: &[(&str, &str)] = &[
    ("version", "Print the current Effigy version"),
    ("fmt", "Format only the named Rust files with their nearest rustfmt config"),
    (
        "fmt-rust",
        "Format named Rust files with the built-in formatter when a repository task owns `effigy fmt`",
    ),
    (
        "rhai",
        "Inspect Effigy's in-process Rhai host API surface",
    ),
    (
        "skill",
        "Load one explicit skill-owned task catalog and run its tasks",
    ),
    (
        "run",
        "Inspect, follow or stop a task run by its id or key (alias of `runs`)",
    ),
    (
        "runs",
        "List task runs and inspect, follow or stop one by its id or key",
    ),
    (
        "uninstall",
        "Plan or remove Effigy-owned local machine state",
    ),
];

pub(super) fn command_names() -> Vec<&'static str> {
    live_builtin_command_names().collect()
}

/// Every live direct command with the one-line description its menu shows.
pub(super) fn command_rows() -> Vec<(&'static str, &'static str)> {
    command_names()
        .into_iter()
        .map(|name| (name, command_description(name)))
        .collect()
}

/// The description a command's menu row shows: the published built-in task
/// summary when the command has one, else the direct command's own row.
fn command_description(name: &str) -> &'static str {
    BUILTIN_TASKS
        .iter()
        .find(|(task, _)| *task == name)
        .map(|(_, description)| *description)
        .or_else(|| {
            DIRECT_COMMAND_DESCRIPTIONS
                .iter()
                .find(|(command, _)| *command == name)
                .map(|(_, description)| *description)
        })
        .unwrap_or_default()
}

pub(super) fn command_options(command: &str) -> &'static [&'static str] {
    match command {
        "help" => &["--json", "--help", "-h"],
        "version" => &["--json", "--help", "-h"],
        "catalog" => &["cache", "clear", "--repo", "--json", "--help", "-h"],
        "tasks" => &[
            "--all",
            "--repo",
            "--task",
            "--resolve",
            "status",
            "--json",
            "--pretty",
            "--help",
            "-h",
        ],
        "doctor" => &[
            "--repo",
            "--fix",
            "--verbose",
            "--deep",
            "--catalog",
            "--all-catalogs",
            "--refresh",
            "--json",
            "--help",
            "-h",
        ],
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
        "completion" => super::super::surface::COMPLETION_COMMAND_OPTIONS,
        "fmt" => &["rust", "--check", "--repo", "--json", "--help", "-h"],
        "fmt-rust" => &["--check", "--repo", "--json", "--help", "-h"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_first_tokens_are_direct_commands_not_help_group_prefixes() {
        let names = command_names();
        for required in [
            "config",
            "container",
            "release",
            "docs",
            "deps",
            "help",
            "version",
        ] {
            assert!(
                names.contains(&required),
                "completion inventory missing direct command `{required}`"
            );
        }
        for prefix in ["local", "repo", "deliver", "extend", "admin"] {
            assert!(
                !names.contains(&prefix),
                "completion inventory must not treat help-group `{prefix}` as a built-in command"
            );
        }
    }

    #[test]
    fn every_live_command_has_a_menu_description() {
        // The command set comes from the CLI inventory, so a new built-in must
        // bring its description with it rather than render a blank menu row.
        for (name, description) in command_rows() {
            assert!(
                !description.is_empty(),
                "completion command `{name}` has no menu description"
            );
        }
    }

    #[test]
    fn deps_completion_exposes_only_the_contract_surface() {
        assert!(command_names().contains(&"deps"));
        assert_eq!(
            command_options("deps"),
            [
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
            ]
        );
    }
}
