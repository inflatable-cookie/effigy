use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_docs_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &DOCS_HELP)
}

const DOCS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "docs",
    notices: &[
        "Run reusable markdown/documentation validation checks without dropping into shell scripts.",
        "`docs context --sources` routes one query across the repositories that declare `[docs_policy.sources] share = true` under the named directories, grouped per repository and never merged into one ranked list.",
        "`docs context` retrieves bounded exact documentation sections with provenance from the shared graph; it returns source evidence, never a generated summary.",
        "Repo-specific policy should stay in task wiring and flags; these built-ins provide the generic validation engines.",
    ],
    usage: &[
        "effigy docs check links [--repo <PATH>] [<FILE>...] [--json]",
        "effigy docs check json-examples [--repo <PATH>] [--file <PATH>] [--section <TITLE>] [--min-blocks <N>] [--require <TEXT>]... [--require-block <N:TEXT>]... [--json]",
        "effigy docs check headings [--repo <PATH>] <FILE>... --require-heading <TEXT>... [--json]",
        "effigy docs check paths [--repo <PATH>] <PATH>... [--json]",
        "effigy docs check contains [--repo <PATH>] <FILE>... --require <TEXT>... [--json]",
        "effigy docs check forbidden [--repo <PATH>] <FILE>... --forbid <TEXT>... [--json]",
        "effigy docs check index [--repo <PATH>] [--policy-index <NAME>] [--dir <PATH>] [--index <PATH>] [--json]",
        "effigy docs check next-action [--repo <PATH>] [--policy <NAME>] [--json]",
        "effigy docs check workflow-paths [--repo <PATH>] [--dir <PATH>] [--json]",
        "effigy docs add-log-index [--repo <PATH>] <LOG_FILE> [--json]",
        "effigy docs context <QUERY> [--repo <PATH>] [--sources <PATH>] [--only <HANDLE>]... [--max-sections <N>] [--max-bytes <N>] [--max-hops <N>] [--json]",
        "effigy --json docs check links [--repo <PATH>] [<FILE>...]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
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
        ("<PATH>", "Require a file or directory to exist for `check-paths`"),
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
        (
            "<QUERY>",
            "Free-text query for `docs context`; an empty query is a usage error",
        ),
        (
            "--max-sections <N>",
            "Cap returned `docs context` sections (default 8, maximum 32)",
        ),
        (
            "--max-bytes <N>",
            "Cap total `docs context` evidence bytes (default 24000, maximum 100000)",
        ),
        (
            "--max-hops <N>",
            "Cap `docs context` typed-relation traversal depth (default 1, maximum 3)",
        ),
        (
            "--sources <PATH>",
            "Route `docs context` across a `[portfolio]` file (or a directory) naming where opted-in repositories live",
        ),
        (
            "--only <HANDLE>",
            "Restrict `--sources` routing to named repository handles; repeatable",
        ),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable validation payloads"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy docs check links README.md docs/guides/README.md",
        "effigy docs check json-examples",
        "effigy docs context \"documentation graph profile contract\" --sources ~/Dev/projects/portfolio.toml --json",
        "effigy docs check headings docs/guides/024-ci-and-automation-recipes.md --require-heading \"## Vision Alignment\"",
        "effigy docs check paths README.md docs/README.md docs/vision/README.md",
        "effigy docs check contains docs/logs/README.md --require \"Vision Target Delta\"",
        "effigy docs check forbidden AGENTS.md external/setup-effigy/README.md --forbid \"--repo .\"",
        "effigy docs check json-examples --file docs/guides/026-json-payload-examples.md --section \"Completion Candidates\"",
        "effigy docs check index --dir docs/logs --index docs/logs/README.md",
        "effigy docs check index --policy-index vision",
        "effigy docs check next-action --policy vision",
        "effigy docs check workflow-paths",
        "effigy docs add-log-index docs/logs/2026-03/02-160000-my-log.md",
        "effigy docs context \"graph freshness\"",
        "effigy docs context \"release gates\" --max-sections 4 --max-bytes 8000 --max-hops 2",
        "effigy --json docs context \"documentation graph profile\"",
        "effigy --json docs check links README.md",
    ],
};
