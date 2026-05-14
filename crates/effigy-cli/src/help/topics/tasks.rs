use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_tasks_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "tasks",
        &["List discovered task catalogs and task commands, or inspect status for one resolved task; use routing probes only when debugging selector resolution."],
        &[
            "effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]",
            "effigy tasks status <SELECTOR> [--repo <PATH>] [--json]",
            "effigy tasks status --all [--repo <PATH>] [--json]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--task <TASK_NAME>", "Filter output to matching task entries"),
            (
                "--resolve <SELECTOR>",
                "Probe task routing evidence for a selector (for example `<catalog>/task` or `test`)",
            ),
            ("--json", "Render machine-readable task catalog payload"),
            (
                "--pretty <true|false>",
                "When used with --json, toggle pretty formatting (default: true)",
            ),
            (
                "status <SELECTOR>",
                "Show live-or-last-known status for one resolved task selector",
            ),
            (
                "status --all",
                "Show repo-plus-descendant task status inventory, including unknown and stale rows",
            ),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy tasks",
            "effigy tasks --repo /path/to/workspace",
            "effigy tasks --repo /path/to/workspace --task db:reset",
            "effigy tasks status test",
            "effigy tasks status catalog-a/build --json",
            "effigy tasks status --all",
            "effigy tasks --resolve <catalog>/<task>",
            "effigy tasks --json --resolve test",
            "effigy --json tasks --repo /path/to/workspace --task test",
        ],
    )
}
