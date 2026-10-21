use std::collections::BTreeSet;

use crate::command_surface::{general_help_entries_for_group, GeneralHelpEntry, HelpGroup};

use super::super::{HelpRenderer, HelpResult, KeyValue, NoticeLevel, TableSpec};

pub(crate) fn render_general_help<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    renderer.section("Commands")?;
    renderer.notice(
        NoticeLevel::Info,
        "Commands are grouped by job. Use `effigy help <group>` for one group and `effigy help <command>` for command detail.",
    )?;
    renderer.text("")?;
    for group in HelpGroup::ALL {
        render_group_section(renderer, *group, deferred_builtins)?;
    }
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy <built-in-task> --help`, `effigy tasks <helper> --help`, or `effigy config completion --help` for task-specific flags and examples.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Effigy's flags go before the task: `effigy [--json] [--plan] [--repo <PATH>] [--lock-wait-ms <MS>] [--env-schema <PATH>] [--verbose-root] [--run-id <key>] <task> [task args]`. Everything after the task name belongs to the task; `<task> --plan` and `<task> --json` are still read as Effigy's for one release. `--run-id <key>` names the run: a repeat with the same key attaches to it and never starts a second; see `effigy help runs`. `--json` is an output format, never a planning or safety flag.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "A managed task runs under one supervisor: `--headless` (or `EFFIGY_MANAGED_HEADLESS=1`) follows it as text, `--detach` returns once it has started, and closing or killing an attached client stops it. Use `effigy <task> status [--json]`, `logs [process] [--follow]`, and `stop` from another shell.",
    )?;
    renderer.key_values(&[
        KeyValue::new("-h, --help", "Print this help panel"),
        KeyValue::new("--version", "Print the current Effigy version"),
        KeyValue::new("--json", "Render command-envelope JSON for CI/tooling"),
        KeyValue::new(
            "--run-id <key> <task>",
            "Name the run with your own key; a repeat attaches instead of starting another",
        ),
        KeyValue::new(
            "--plan <task>",
            "Inspect the resolved task, catalog, and command without executing",
        ),
        KeyValue::new(
            "--lock-wait-ms <MS> <task>",
            "Wait up to MS for a live run of the same task to finish",
        ),
    ])?;
    Ok(())
}

/// Render one `effigy help <group>` panel.
pub(crate) fn render_help_group<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    group: HelpGroup,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    render_group_section(renderer, group, deferred_builtins)?;
    renderer.notice(
        NoticeLevel::Info,
        "Run these commands directly: help grouping adds discovery only, never an `effigy <group> <command>` route.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Use `effigy help <command>` for command detail, or `effigy help` for every group.",
    )?;
    Ok(())
}

fn render_group_section<R: HelpRenderer + ?Sized>(
    renderer: &mut R,
    group: HelpGroup,
    deferred_builtins: &BTreeSet<String>,
) -> HelpResult<()> {
    renderer.section(group.title())?;
    renderer.text(group.summary())?;
    renderer.text("")?;
    renderer.table(&TableSpec::new(
        Vec::new(),
        visible_group_rows(group, deferred_builtins)
            .map(|entry| vec![entry.command.to_owned(), entry.description.to_owned()])
            .collect::<Vec<Vec<String>>>(),
    ))?;
    renderer.text("")?;
    Ok(())
}

fn visible_group_rows<'a>(
    group: HelpGroup,
    deferred_builtins: &'a BTreeSet<String>,
) -> impl Iterator<Item = &'static GeneralHelpEntry> + 'a {
    general_help_entries_for_group(group).filter(|entry| {
        entry
            .deferred_builtin
            .is_none_or(|name| !deferred_builtins.contains(name))
    })
}
