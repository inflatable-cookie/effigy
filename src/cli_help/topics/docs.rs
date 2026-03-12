use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

pub(crate) fn render_docs_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("docs Help")?;
    render_info_notices(
        renderer,
        &[
            "Run reusable markdown/documentation validation checks without dropping into shell scripts.",
            "Repo-specific policy should stay in task wiring and flags; these built-ins provide the generic validation engines.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy docs check-links [--repo <PATH>] [<FILE>...] [--json]",
            "effigy docs check-json-examples [--repo <PATH>] [--file <PATH>] [--section <TITLE>] [--min-blocks <N>] [--require <TEXT>]... [--require-block <N:TEXT>]... [--json]",
            "effigy docs check-index [--repo <PATH>] [--dir <PATH>] [--index <PATH>] [--json]",
            "effigy docs check-workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]",
            "effigy docs add-log-index [--repo <PATH>] <LOG_FILE> [--json]",
            "effigy --json docs check-links [--repo <PATH>] [<FILE>...]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--file <PATH>",
                "Override the markdown file scanned by `check-json-examples`",
            ),
            (
                "--section <TITLE>",
                "Override the target `##` section heading for JSON example checks",
            ),
            (
                "--min-blocks <N>",
                "Require at least N fenced `json` blocks in the selected section",
            ),
            (
                "--require <TEXT>",
                "Require a substring in every captured JSON example block",
            ),
            (
                "--require-block <N:TEXT>",
                "Require a substring in one specific 1-based JSON example block",
            ),
            (
                "--dir <PATH>",
                "Override the directory scanned by `check-index` or `check-workflow-paths`",
            ),
            (
                "--index <PATH>",
                "Override the markdown index file checked by `check-index`",
            ),
            (
                "<LOG_FILE>",
                "Log path to insert into `docs/logs/README.md` for `add-log-index`",
            ),
            ("--json", "Render machine-readable validation payloads"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy docs check-links README.md docs/guides/README.md",
            "effigy docs check-json-examples",
            "effigy docs check-json-examples --file docs/guides/026-json-payload-examples.md --section \"13) Completion Candidates\"",
            "effigy docs check-index --dir docs/logs --index docs/logs/README.md",
            "effigy docs check-workflow-paths --repo .",
            "effigy docs add-log-index docs/logs/2026-03/02-160000-my-log.md",
            "effigy --json docs check-links README.md",
        ],
    )?;
    Ok(())
}
