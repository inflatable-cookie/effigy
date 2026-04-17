use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_tasks_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("tasks Help")?;
    render_info_notices(
        renderer,
        &["List discovered task catalogs and task commands; use routing probes only when debugging selector resolution."],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]",
        ],
    )?;
    render_options_section(
        renderer,
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
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy tasks",
            "effigy tasks --repo /path/to/workspace",
            "effigy tasks --repo /path/to/workspace --task db:reset",
            "effigy tasks --resolve <catalog>/<task>",
            "effigy tasks --json --resolve test",
            "effigy --json tasks --repo /path/to/workspace --task test",
        ],
    )?;
    Ok(())
}
