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
    renderer.text("")?;
    render_program_fallbacks(renderer)?;
    Ok(())
}

/// The mandatory disclosure for `[programs.<name>]` (Q-008). Every condition
/// below is acceptance, not decoration: the trigger, `force`'s inherited-PATH
/// scope, the optional allow list, exact argv with no translation or retry,
/// and the loss of the original program's safeguards.
fn render_program_fallbacks<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("Program Fallbacks")?;
    renderer.text(
        "`[programs.<name>]` in the Effigy user config (`~/.effigy/config.toml`), never a project `effigy.toml`: `fallback = \"<program>\"`, optional `allow = [\"<first argument>\", ...]`, optional `force = true` (default `false`).",
    )?;
    renderer.bullet_list(
        "Conditions",
        &[
            "Trigger: the fallback is active when `<name>` is not an executable on the inherited PATH, or when its block sets `force = true`. A per-run shim named `<name>` then comes first on that run's PATH and the run prints an honest notice.".to_owned(),
            "Scope: only name resolution through the inherited PATH changes. `force = true` changes nothing else: absolute paths, a replaced PATH (including a task `env` PATH), shell aliases and shell functions still reach the real program or nothing.".to_owned(),
            "Allow list: omitted means unrestricted exact argv. Present means only those exact first arguments run; anything else, including no argument, is refused before exec and the fallback never starts. Effigy interprets no global options, `--` or subcommands: the first argument is matched literally.".to_owned(),
            "Exact argv: the shim execs the fallback with the arguments it received. Effigy translates no flags, adds nothing, and never retries after the fallback fails; the fallback's exit status is the shim's.".to_owned(),
            "Lost safeguards: the original program's behaviour is gone. For `oxide` -> `cargo`, `oxide clean` becomes `cargo clean` without Oxide's ownership checks or managed storage, and cargo plugins and cargo-subcommand exec run as plain cargo would. The fallback is resolved to an absolute path on the inherited PATH, so it never re-enters a shim and fallbacks never chain.".to_owned(),
        ],
    )?;
    renderer.text("")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        lines: Vec<String>,
    }

    impl HelpRenderer for Capture {
        fn text(&mut self, body: &str) -> HelpResult<()> {
            self.lines.push(body.to_owned());
            Ok(())
        }
        fn section(&mut self, title: &str) -> HelpResult<()> {
            self.lines.push(title.to_owned());
            Ok(())
        }
        fn notice(&mut self, _level: NoticeLevel, body: &str) -> HelpResult<()> {
            self.lines.push(body.to_owned());
            Ok(())
        }
        fn bullet_list(&mut self, title: &str, items: &[String]) -> HelpResult<()> {
            self.lines.push(title.to_owned());
            self.lines.extend(items.iter().cloned());
            Ok(())
        }
        fn table(&mut self, _spec: &TableSpec) -> HelpResult<()> {
            Ok(())
        }
        fn key_values(&mut self, _items: &[KeyValue]) -> HelpResult<()> {
            Ok(())
        }
    }

    /// Q-008's acceptance condition: `effigy help` states the trigger,
    /// `force`'s inherited-PATH scope, the optional allow list, exact argv with
    /// no translation or retry, and the lost safeguards.
    #[test]
    fn general_help_states_every_program_fallback_condition() {
        let mut capture = Capture::default();
        render_general_help(&mut capture, &BTreeSet::new()).expect("render general help");
        let help = capture.lines.join("\n");
        for token in [
            "Program Fallbacks",
            "`[programs.<name>]`",
            "not an executable on the inherited PATH",
            "`force = true`",
            "unrestricted exact argv",
            "exact first arguments",
            "refused before exec",
            "exact argv",
            "never retries",
            "`oxide clean` becomes `cargo clean`",
            "cargo plugins and cargo-subcommand exec",
            "absolute paths, a replaced PATH",
            "shell aliases and shell functions",
        ] {
            assert!(help.contains(token), "`effigy help` is missing {token}");
        }
    }
}
