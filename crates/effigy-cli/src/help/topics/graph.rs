use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_graph_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &GRAPH_HELP)
}

const GRAPH_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "graph",
    notices: &[
        "Build and query a deterministic local code graph for agent-facing navigation.",
        "The graph stays local under `.effigy/graph/`; queries do not rebuild it implicitly.",
    ],
    usage: &[
        "effigy graph index [--repo <PATH>] [--json]",
        "effigy graph status [--repo <PATH>] [--json]",
        "effigy graph watch [--repo <PATH>] [--debounce-ms <MS>] [--json]",
        "effigy graph search [--repo <PATH>] [--limit <N>] <QUERY> [--json]",
        "effigy graph files [--repo <PATH>] [--limit <N>] [--json]",
        "effigy graph node [--repo <PATH>] <ID> [--json]",
        "effigy graph callers [--repo <PATH>] [--limit <N>] <ID> [--json]",
        "effigy graph callees [--repo <PATH>] [--limit <N>] <ID> [--json]",
        "effigy graph impact [--repo <PATH>] [--limit <N>] <TARGET> [--json]",
        "effigy graph context [--repo <PATH>] [--max-files <N>] [--max-bytes <N>] [--language <ID>]... [--path <PREFIX>]... <REQUEST> [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("--debounce-ms <MS>", "Delay incremental refresh until the repo is quiet for the given milliseconds"),
        ("--limit <N>", "Cap bounded query output for search, files, callers, callees, and impact"),
        ("--max-files <N>", "Cap selected files returned by `graph context`"),
        ("--max-bytes <N>", "Cap total snippet bytes returned by `graph context`"),
        ("--language <ID>", "Restrict `graph context` output to one language id"),
        ("--path <PREFIX>", "Restrict `graph context` output to matching path prefixes"),
        ("<QUERY>", "Text query for FTS-backed graph search"),
        ("<ID>", "Stable graph node identifier"),
        ("<TARGET>", "Path, symbol id, or other graph target for impact analysis"),
        ("<REQUEST>", "Natural-language task request for bounded context-pack selection"),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable graph payloads"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy graph index",
        "effigy graph status --json",
        "effigy graph watch --debounce-ms 1000 --json",
        "effigy graph search deploy --limit 20 --json",
        "effigy graph node symbol:rust:run_release --json",
        "effigy graph callers symbol:rust:run_release --json",
        "effigy graph impact src/runner/release_command/mod.rs --json",
        "effigy graph context \"trace deploy provider export\" --max-files 8 --json",
    ],
};
