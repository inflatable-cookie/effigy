use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_tasks_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &TASKS_HELP)
}

const TASKS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "tasks",
    notices: &["List public tasks grouped by `group` (ungrouped tasks use General). `--all` and `--json` include internal tasks; JSON rows report effective visibility, group, and visibility_reason. Internal tasks still run directly by name.", "Bounded QA groups (contract 051) live under the exact multiword `tasks qa-groups` / `tasks qa-group` forms; they reserve no top-level selector, never fall through to a task with the same name, and `stop`/`hard_timeout_ms` stay unavailable until owned-run supervision (contract 052) lands.", "`tasks --json` also carries a portable `selectors` inventory: one deterministic array of invocation-ready names (`effigy <selector>`) with `kind` (`task`, `managed-profile`, `builtin`) and `source` (`catalog`, `builtin`), so tooling can query `.result.selectors` instead of flattening `catalog_tasks`, `managed_profiles`, and `builtin_tasks`."],
    usage: &[
        "effigy tasks [--all] [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]",
        "effigy tasks status <SELECTOR> [--repo <PATH>] [--json]",
        "effigy tasks status --all [--repo <PATH>] [--json]",
        "effigy tasks qa-groups list [FILTER] [--file PATH] [--json]",
        "effigy tasks qa-group run <SELECTOR> [--file PATH] [--scope TOKEN]... [--plan] [--json]",
        "effigy tasks qa-group status <RUN_ID> [--json]",
        "effigy tasks qa-group logs <RUN_ID> [--follow]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("--all", "Include internal tasks in the text inventory; JSON always includes every task"),
        ("--task <TASK_NAME>", "Filter output to matching task entries"),
        (
            "--resolve <SELECTOR>",
            "Show where a selector goes, exactly as running it would: typed command, task, built-in, exec alias or `[defer]`",
        ),
        (
            "--pretty <true|false>",
            "When used with --json, toggle pretty formatting (default: true)",
        ),
        (
            "status <SELECTOR>",
            "Show the live run (held gate) or the last run of one resolved task, from the run journal",
        ),
        (
            "status --all",
            "Show every declared task's state, plus live runs of tasks no longer declared",
        ),
        (
            "qa-groups list [FILTER]",
            "List maintained QA groups (plus the one --file temporary definition); no directory scan exists",
        ),
        (
            "qa-group run <SELECTOR>",
            "Resolve one QA group and run every member sequentially through the standard pipeline; --plan resolves without executing, and a needs_planner scope never creates a run",
        ),
        (
            "qa-group status <RUN_ID>",
            "Show live-or-final QA-group run evidence; a run whose owner exited stays unknown, never a pass",
        ),
        (
            "qa-group logs <RUN_ID>",
            "Print the run-scoped member logs (pipeline-redacted captures)",
        ),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable task catalog payload"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy tasks",
        "effigy tasks --repo /path/to/workspace",
        "effigy tasks --repo /path/to/workspace --task db:reset",
        "effigy tasks status test",
        "effigy tasks status catalog-a/build --json",
        "effigy tasks status --all",
        "effigy tasks --resolve <catalog>/<task>",
        "effigy tasks --json --resolve test",
        "effigy --json tasks --repo /path/to/workspace --task test",
        "effigy tasks qa-groups list",
        "effigy tasks qa-group run <group> --scope path:crates/x/src/lib.rs --plan",
        "effigy tasks qa-group run <group> --file config/qa-groups/2026-10-02-check.toml",
    ],
};
