use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_deploy_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "deploy",
        &[
            "Inspect the provider-neutral production deployment model derived from the effective manifest and bundle state.",
            "The model/export surfaces remain Underlay-first. Export remains file-oriented. Deployment transactions compose code refs, state stacks, artifact policy, release evidence, provider adapters, hooks, health checks, and reports.",
        ],
        &[
            "effigy deploy model [--repo <PATH>] --json",
            "effigy deploy export render [--repo <PATH>] --path <DIR> [--plan] [--json]",
            "effigy deploy export railway [--repo <PATH>] --path <DIR> [--plan] [--json]",
            "effigy deploy plan <ENV> [--repo <PATH>] [--write-report] [--json]",
            "effigy deploy apply <ENV> [--repo <PATH>] --yes [--json]",
            "effigy deploy status <ENV> [--repo <PATH>] [--json]",
            "effigy deploy history <ENV> [--repo <PATH>] [--limit <N>] [--json]",
            "effigy deploy redeploy <ENV> [--repo <PATH>] --deployment <ID> --yes [--json]",
            "effigy --json deploy model [--repo <PATH>]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--path <DIR>", "Write export files under this directory"),
            ("--plan", "Preview the export without writing files"),
            ("--write-report", "Persist deploy plan output under .effigy/reports/deploy/<env>/"),
            ("--deployment <ID>", "Select a deployment history entry for redeploy"),
            ("--limit <N>", "Limit deployment history results"),
            ("--yes", "Execute an apply or redeploy transaction after review"),
            ("--json", "Render machine-facing deploy payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy deploy model --json",
            "effigy deploy export render --path infra/render --plan",
            "effigy deploy export railway --path infra/railway --plan",
            "effigy deploy plan uat --write-report",
            "effigy deploy apply uat --yes",
            "effigy deploy history production --limit 5",
            "effigy --json deploy model --repo /path/to/repo",
        ],
    )
}
