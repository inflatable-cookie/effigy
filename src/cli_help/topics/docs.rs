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
            "effigy docs check-headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]",
            "effigy docs check-contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]",
            "effigy docs check-forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]",
            "effigy docs check-index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]",
            "effigy docs check-next-action [--repo <PATH>] [--policy <NAME>] [--json]",
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
                "--require-heading <TEXT>",
                "Require a heading to appear in every file passed to `check-headings`",
            ),
            (
                "--require <TEXT>",
                "Require a substring in every file passed to `check-contains`",
            ),
            (
                "--forbid <TEXT>",
                "Require a substring to be absent from every file passed to `check-forbidden`",
            ),
            (
                "--policy-index <NAME>",
                "Use a named `[docs_policy.indexes.<NAME>]` definition from `effigy.toml`",
            ),
            (
                "--policy <NAME>",
                "Use a named `[docs_policy.next_actions.<NAME>]` definition from `effigy.toml`",
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
            "effigy docs check-headings docs/guides/024-ci-and-automation-recipes.md --require-heading \"## Vision Alignment\"",
            "effigy docs check-contains docs/logs/README.md --require \"Vision Target Delta\"",
            "effigy docs check-forbidden AGENTS.md setup-effigy/README.md --forbid \"--repo .\"",
            "effigy docs check-json-examples --file docs/guides/026-json-payload-examples.md --section \"13) Completion Candidates\"",
            "effigy docs check-index --dir docs/logs --index docs/logs/README.md",
            "effigy docs check-index --policy-index vision",
            "effigy docs check-next-action --policy vision",
            "effigy docs check-workflow-paths",
            "effigy docs add-log-index docs/logs/2026-03/02-160000-my-log.md",
            "effigy --json docs check-links README.md",
        ],
    )?;
    Ok(())
}
