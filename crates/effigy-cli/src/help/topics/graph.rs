use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_graph_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &GRAPH_HELP)
}

const GRAPH_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "graph",
    notices: &[
        "Build and query one local code graph for the whole checkout under `.effigy/graph/`.",
        "Data queries read committed files without writes or refreshes; run `graph index` when a packet reports stale or missing data.",
        "Status always reports without writing; the legacy `--refresh` flag warns and stays read-only.",
        "Queries have a 120000ms cooperative deadline by default; `EFFIGY_GRAPH_TIMEOUT_MS=<MS>` overrides it and `0` disables it. Explicit index is unbounded.",
        "Each file commits graph rows, search data and freshness together. Concurrent reads report `refreshing (n/N)` and read committed files immediately.",
        "Pass `--stale-index` to label committed data explicitly stale. A timeout stops at a safe boundary and preserves progress for the next refresh.",
        "Use `graph affected` after edits to narrow likely validation targets; confirm exact tokens with `rg`.",
    ],
    usage: &[
        "effigy graph index [--repo <PATH>] [--json]",
        "effigy graph status [--repo <PATH>] [--refresh] [--json]",
        "effigy graph search <QUERY> [--repo <PATH>] [--limit <N>] [--stale-index] [--json]",
        "effigy graph node <ID|PATH> [--repo <PATH>] [--in|--out] [--limit <N>] [--stale-index] [--json]",
        "effigy graph affected <PATH>... [--repo <PATH>] [--depth <N>] [--limit <N>] [--stdin] [--stale-index] [--json]",
        "effigy graph explore <REQUEST> [--repo <PATH>] [--excerpts-only] [--max-files <N>] [--max-bytes <N>] [--language <ID>]... [--path <PREFIX>]... [--stale-index] [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("--refresh", "Deprecated compatibility flag; warns and returns read-only status"),
        ("--stale-index", "Read committed files with stale-index or refreshing trust"),
        ("EFFIGY_GRAPH_TIMEOUT_MS=<MS>", "Set the cooperative query deadline; 0 disables it"),
        ("--in", "Show incoming relationships and callers for node"),
        ("--out", "Show outgoing relationships and callees for node; omit both flags for both directions"),
        ("--limit <N>", "Bound search results, each node result collection, or affected results"),
        ("--depth <N>", "Cap traversal depth for affected"),
        ("--stdin", "Read newline-delimited changed paths for affected"),
        ("--excerpts-only", "Return the bounded excerpt packet from explore"),
        ("--max-files <N>", "Cap selected files in explore"),
        ("--max-bytes <N>", "Cap snippet bytes in explore"),
        ("--language <ID>", "Restrict explore to a language; repeatable"),
        ("--path <PREFIX>", "Restrict explore to path prefixes; repeatable"),
        ("<ID|PATH>", "Stable symbol or file id, or repository-relative file path"),
    ],
    trailing_common_options: &[CommonOption::Json("Render machine-readable graph payloads"), CommonOption::Help],
    examples: &[
        "effigy graph status --json",
        "effigy graph index",
        "effigy graph explore \"trace deploy provider export\" --max-files 8 --json",
        "effigy graph explore \"trace deploy provider export\" --excerpts-only --json",
        "effigy graph search deploy --limit 20 --json",
        "effigy graph node src/runner/release_command/mod.rs --json",
        "effigy graph node symbol:rust:run_release --in --json",
        "git diff --name-only | effigy graph affected --stdin --json",
        "mv .effigy/graph .effigy/graph.backup-$(date +%s) && effigy graph index --json",
    ],
};
