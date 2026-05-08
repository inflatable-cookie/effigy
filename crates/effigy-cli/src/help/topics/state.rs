use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_state_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "state",
        &[
            "Plan, apply, capture, and inspect layered state-stack reports without moving app semantics into Effigy.",
            "State stacks validate `effigy.state-stack.v1`, keep app transforms in repo-owned tasks, and write operator-visible lineage history.",
        ],
        &[
            "effigy state plan [<STACK>] [--repo <PATH>] [--json] [--write-report]",
            "effigy state plan --manifest <PATH> [--repo <PATH>] [--json] [--write-report]",
            "effigy state plan --stack <NAME> [--repo <PATH>] [--json] [--write-report]",
            "effigy state apply [<STACK>] [--yes] [--json]",
            "effigy state capture <STACK> <PROFILE> [--yes] [--push] [--json]",
            "effigy state capture [<STACK>] --role <ROLE> --source-env <ENV> --key <KEY> [--json]",
            "effigy state capture [<STACK>] --role <ROLE> --source-env <ENV> --key <KEY> --source <PATH> --ref oci://<REF> --yes [--push] [--json]",
            "effigy state history [<STACK>] [--kind plan|apply|capture] [--limit <N>] [--lineage <ID>] [--json]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--manifest <PATH>", "Standalone state-stack manifest path, equivalent to the positional argument"),
            ("--stack <NAME>", "Select a stack from `[state.<NAME>]` in the composed manifest"),
            ("--role <ROLE>", "Capture role, currently `uat-capture` or `full-capture`"),
            ("--source-env <ENV>", "Source environment label for a planned capture"),
            ("--key <KEY>", "Produced state-stack layer key for a planned capture"),
            ("--source <PATH>", "Already-produced local capture payload to stage when `--yes` is supplied"),
            ("--ref <REF>", "Optional planned local or OCI destination ref for the captured layer"),
            ("--push", "Publish the captured artifact to the explicit OCI ref after local staging"),
            ("--kind <KIND>", "Filter history by report kind: `plan`, `apply`, or `capture`"),
            ("--limit <N>", "Limit state history results"),
            ("--lineage <ID>", "Filter reports by `lineage_id` or `parent_lineage_id`"),
            ("--hook <TASK>", "Optional apply hook to record on the produced layer"),
            ("--task <TASK>", "Optional repo-owned capture task to report as planned"),
            ("--write-report", "Write the lineage report to `.effigy/reports/state/<stack>/plan.json`"),
            ("--yes", "Execute supported state apply layers, or stage a state capture payload when used with `state capture --source`"),
            ("--json", "Render machine-readable state-stack lineage payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy state plan",
            "effigy state plan --write-report",
            "effigy state plan uat",
            "effigy state apply uat",
            "effigy state apply uat --yes",
            "effigy state capture uat new-content --yes",
            "effigy state capture uat --role uat-capture --source-env uat --key uat-capture-2026-05-08 --json",
            "effigy state capture --stack uat --role uat-capture --source-env uat --key uat-capture-2026-05-08 --source ./captures/uat.tar --ref oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08 --yes --json",
            "effigy state capture --stack uat --role uat-capture --source-env uat --key uat-capture-2026-05-08 --source ./captures/uat.tar --ref oci://ghcr.io/acowtancy/content:uat-capture-2026-05-08 --yes --push --json",
            "effigy state history uat --kind capture --limit 5 --json",
            "effigy state plan --manifest state-stack.toml",
            "effigy state plan ./ops/acowtancy-uat.state.toml --json",
        ],
    )
}
