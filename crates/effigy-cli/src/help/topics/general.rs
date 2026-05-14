use std::collections::BTreeSet;

use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel, TableSpec};

pub(crate) fn render_general_help<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    let commands = crate::help::general_help_command_rows()
        .chain([
            (
                "effigy version",
                "Print the current Effigy version (same as --version)",
                None,
            ),
            (
                "effigy tasks migrate",
                "Import package scripts into `[tasks]` with preview/apply flow",
                None,
            ),
            (
                "effigy tasks unlock",
                "Manually clear lock scopes (`workspace`, `shared:*`, `task:*`, `profile:*/*`)",
                None,
            ),
            (
                "effigy tasks cache",
                "Inspect/invalidate phase-1 task cache metadata (`inspect`, `invalidate`)",
                None,
            ),
            (
                "effigy config",
                "Show config keys/examples, bundle schema guidance, or inspect the effective composed manifest and focused path sources",
                None,
            ),
            (
                "effigy config completion",
                "Generate shell completion scripts and selector candidates",
                None,
            ),
            (
                "effigy scan",
                "Run built-in repository scanners such as `god-files` and `attention-markers`",
                None,
            ),
            ("effigy <task>", "Resolve task across discovered catalogs", None),
            (
                "effigy <catalog>/<task>",
                "Run task from explicit catalog alias",
                None,
            ),
        ])
        .collect::<Vec<_>>();
    renderer.section("Commands")?;
    renderer.table(&TableSpec::new(
        Vec::new(),
        commands
            .into_iter()
            .filter(|(_, _, builtin)| builtin.is_none_or(|name| !deferred_builtins.contains(name)))
            .map(|(command, description, _)| vec![command.to_owned(), description.to_owned()])
            .collect::<Vec<Vec<String>>>(),
    ))?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy <built-in-task> --help`, `effigy tasks <helper> --help`, or `effigy config completion --help` for task-specific flags and examples.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Global `--json` and `--repo <PATH>` now work before built-ins and task selectors. Generic task invocations also accept `--verbose-root` and `--env-schema <PATH>` before the selector.",
    )?;
    renderer.key_values(&[
        KeyValue::new("-h, --help", "Print this help panel"),
        KeyValue::new("--version", "Print the current Effigy version"),
        KeyValue::new("--json", "Render command-envelope JSON for CI/tooling"),
    ])?;
    Ok(())
}
