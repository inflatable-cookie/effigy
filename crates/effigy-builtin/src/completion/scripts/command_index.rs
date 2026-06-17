use super::super::surface::COMPLETION_COMMAND_OPTIONS;
use crate::constants::BUILTIN_TASKS;

pub(super) fn command_names() -> Vec<&'static str> {
    let mut names = Vec::with_capacity(BUILTIN_TASKS.len() + 2);
    names.push("help");
    names.push("version");
    for (name, _) in BUILTIN_TASKS {
        names.push(name);
    }
    names
}

pub(super) fn command_rows() -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![
        ("help", "Show general help"),
        ("version", "Print the current Effigy version"),
    ];
    rows.extend(BUILTIN_TASKS.iter().copied());
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
        _ => &[],
    }
}
