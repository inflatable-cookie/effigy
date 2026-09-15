use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_graph_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &GRAPH_HELP)
}

const GRAPH_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "graph",
    notices: &[
        "Build and query a deterministic local code graph for agent-facing navigation.",
        "The graph stays local under `.effigy/graph/`; data queries refresh a stale or missing index on demand.",
        "Use `graph status` only for a report-only freshness check; start code understanding with `graph explore` or `graph context` directly.",
        "Pass `graph status --refresh` when the status report should also rebuild a stale or missing index.",
        "Graph data queries have a 120000ms wall-clock budget by default; set `EFFIGY_GRAPH_TIMEOUT_MS=<MS>` to override it, or use `0` to disable the bound. Explicit index and watch commands are unbounded.",
        "Use `graph affected` after edits to narrow likely validation targets before widening to full-suite checks.",
        "Treat graph packets as bounded guidance; exact-token confirmation still belongs to `rg`.",
        "`graph watch --json` streams newline-delimited `effigy.graph.watch.event.v1` payloads instead of a single command envelope.",
        "A segmented catalog is an independently lazy graph scope: `--catalog <ALIAS>` selects one, invocation cwd inside a segmented catalog selects the deepest match, and plain commands otherwise use the root scope. `--all-catalogs` is the only fan-out.",
    ],
    usage: &[
        "effigy graph index [--repo <PATH>] [--catalog <ALIAS>] [--all-catalogs] [--json]",
        "effigy graph status [--repo <PATH>] [--refresh] [--catalog <ALIAS>] [--all-catalogs] [--json]",
        "effigy graph watch [--repo <PATH>] [--debounce-ms <MS>] [--catalog <ALIAS>] [--json]",
        "effigy graph search [--repo <PATH>] [--limit <N>] [--catalog <ALIAS>] [--all-catalogs] <QUERY> [--json]",
        "effigy graph files [--repo <PATH>] [--limit <N>] [--catalog <ALIAS>] [--all-catalogs] [--json]",
        "effigy graph node [--repo <PATH>] [--catalog <ALIAS>] [--all-catalogs] <ID> [--json]",
        "effigy graph callers [--repo <PATH>] [--limit <N>] [--catalog <ALIAS>] [--all-catalogs] <ID> [--json]",
        "effigy graph callees [--repo <PATH>] [--limit <N>] [--catalog <ALIAS>] [--all-catalogs] <ID> [--json]",
        "effigy graph impact [--repo <PATH>] [--limit <N>] [--catalog <ALIAS>] [--all-catalogs] <TARGET> [--json]",
        "effigy graph affected [--repo <PATH>] [--depth <N>] [--limit <N>] [--catalog <ALIAS>] [--all-catalogs] [--stdin] <PATH>... [--json]",
        "effigy graph context [--repo <PATH>] [--max-files <N>] [--max-bytes <N>] [--catalog <ALIAS>] [--all-catalogs] [--language <ID>]... [--path <PREFIX>]... <REQUEST> [--json]",
        "effigy graph explore [--repo <PATH>] [--max-files <N>] [--max-bytes <N>] [--catalog <ALIAS>] [--all-catalogs] [--language <ID>]... [--path <PREFIX>]... <REQUEST> [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("--catalog <ALIAS>", "Select one effective segmented catalog instead of the cwd-selected or root scope"),
        ("--all-catalogs", "Explicit per-catalog fan-out over the root scope and every segmented catalog"),
        ("--debounce-ms <MS>", "Delay incremental refresh until the repo is quiet for the given milliseconds"),
        ("--refresh", "Rebuild a stale or missing index for `graph status` instead of reporting only"),
        ("EFFIGY_GRAPH_TIMEOUT_MS=<MS>", "Override the graph data-query wall-clock budget; `0` disables it (index and watch are unbounded)"),
        ("--depth <N>", "Cap graph traversal depth for `graph affected`"),
        ("--limit <N>", "Cap bounded query output for search, files, callers, callees, and impact"),
        ("--stdin", "Read newline-delimited changed paths from stdin for `graph affected`"),
        ("--max-files <N>", "Cap selected files returned by `graph context` and `graph explore`"),
        ("--max-bytes <N>", "Cap total snippet bytes returned by `graph context` and `graph explore`"),
        ("--language <ID>", "Restrict `graph context` and `graph explore` output to one language id"),
        ("--path <PREFIX>", "Restrict `graph context` and `graph explore` output to matching path prefixes"),
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
        "effigy graph status --json",
        "effigy graph index",
        "effigy graph index --json && effigy graph status --json",
        "effigy graph context \"trace deploy provider export\" --max-files 8 --json",
        "effigy graph explore \"trace graph watch implementation\" --max-files 6 --json",
        "effigy graph search deploy --limit 20 --json",
        "effigy graph impact src/runner/release_command/mod.rs --json",
        "git diff --name-only | effigy graph affected --stdin --json",
        "mv .effigy/graph .effigy/graph.backup-$(date +%s) && effigy graph index --json",
        "effigy graph watch --debounce-ms 1000 --json",
        "effigy graph node symbol:rust:run_release --json",
        "effigy graph callers symbol:rust:run_release --json",
        "effigy graph explore \"bovine desktop query\" --catalog bovine-desktop --json",
        "effigy graph index --all-catalogs --json",
    ],
};
