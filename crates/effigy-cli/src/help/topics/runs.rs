use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    option_rows, render_standard_topic_help_spec, text_lines, CommonOption, StandardTopicHelpSpec,
};

pub(crate) fn render_runs_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &RUNS_HELP)
}

const RUNS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "runs",
    notices: text_lines![
        "Every task run has an identity in the kernel run journal, heavy or not.",
        "`effigy --run-id <key> <task>` names a run with your own key. A repeat with the same key attaches to that run and exits with its result; it never starts a second run.",
        "A run reads by its run id or its key. A run started with `--run-id` logs each stream's output (redacted) unless both were terminals; other runs keep no log.",
        "`run logs --follow` ends when the run settles. Resume a lost follow with the byte offsets you already received; expired bytes are reported, never skipped.",
        "`run stop` asks the run's owner to stop through the kernel journal; the owner stops the task's whole process group and settles the run.",
        "`run migrate` only reports. v0.15 imports v0.14.1 legacy state on first use; the retired current-main central store, any cutover manifest and any old import record are ignored and never deleted."
    ],
    usage: text_lines![
        "effigy run migrate --plan [--json]",
        "effigy runs [--limit <N>] [--all] [--json]",
        "effigy run status <run> [--json]",
        "effigy run logs <run> [--follow] [--stdout-offset <N>] [--stderr-offset <N>] [--json]",
        "effigy run stop <run> [--wait-ms <MS>] [--json]",
        "effigy --run-id <key> <task> [task args]",
    ],
    leading_common_options: &[],
    options: RUNS_OPTIONS,
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable run payloads; `run logs --json` streams one frame per line"),
        CommonOption::Help,
    ],
    examples: text_lines![
        "effigy --run-id ci-1234 test",
        "effigy run status ci-1234",
        "effigy run logs ci-1234 --follow",
        "effigy run logs ci-1234 --follow --stdout-offset 5120 --stderr-offset 96",
        "effigy run stop ci-1234",
    ],
};

const RUNS_OPTIONS: &[(&str, &str)] = option_rows![
    "--plan" => "Report runtime migration state without creating, importing or changing anything",
    "--limit <N>" => "Runs to list, newest first (default 20)",
    "--all" => "List runs from every checkout on this host, not only this one",
    "--follow" => "Keep reading until the run settles, then exit with its result",
    "--stdout-offset <N>" => "Resume stdout at this byte offset",
    "--stderr-offset <N>" => "Resume stderr at this byte offset",
    "--wait-ms <MS>" => "How long `run stop` waits for the run to settle (default 30000)",
];
