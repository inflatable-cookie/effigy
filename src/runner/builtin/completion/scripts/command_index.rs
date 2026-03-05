use super::super::super::super::BUILTIN_TASKS;
use super::super::surface::COMPLETION_COMMAND_OPTIONS;

pub(super) fn command_names() -> Vec<&'static str> {
    let mut names = Vec::with_capacity(BUILTIN_TASKS.len() + 1);
    names.push("help");
    for (name, _) in BUILTIN_TASKS {
        names.push(name);
    }
    names
}

pub(super) fn command_rows() -> Vec<(&'static str, &'static str)> {
    let mut rows = vec![("help", "Show general help")];
    rows.extend(BUILTIN_TASKS.iter().copied());
    rows
}

pub(super) fn command_options(command: &str) -> &'static [&'static str] {
    match command {
        "help" => &["--json", "--help", "-h"],
        "tasks" | "catalogs" => &[
            "--repo",
            "--task",
            "--resolve",
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
        "init" => &["--dry-run", "--force", "--json", "--help", "-h"],
        "migrate" => &["--from", "--script", "--apply", "--json", "--help", "-h"],
        "config" => &[
            "--schema",
            "--minimal",
            "--target",
            "--runner",
            "--json",
            "--help",
            "-h",
        ],
        "unlock" => &["--all", "--json", "--help", "-h"],
        "cache" => &["inspect", "invalidate", "--all", "--json", "--help", "-h"],
        "completion" => COMPLETION_COMMAND_OPTIONS,
        _ => &[],
    }
}
