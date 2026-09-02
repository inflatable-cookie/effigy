use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_deploy_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "deploy",
        &[
            "Inspect the provider-neutral production deployment model derived from the effective manifest and bundle state.",
            "Export is provider-package driven and file-oriented. Deployment transactions compose code refs, state stacks, artifact policy, release evidence, provider adapters, hooks, health checks, and reports.",
        ],
        &[
            "effigy deliver deploy model [--repo <PATH>] --json",
            "effigy deliver deploy export <PROVIDER> [--repo <PATH>] --path <DIR> [--plan] [--json]",
            "effigy deliver deploy plan <ENV> [--repo <PATH>] [--write-report] [--json]",
            "effigy deliver deploy apply <ENV> [--repo <PATH>] --yes [--json]",
            "effigy deliver deploy status <ENV> [--repo <PATH>] [--json]",
            "effigy deliver deploy history <ENV> [--repo <PATH>] [--limit <N>] [--json]",
            "effigy deliver deploy redeploy <ENV> [--repo <PATH>] --deployment <ID> --yes [--json]",
            "effigy --json deliver deploy model [--repo <PATH>]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("<PROVIDER>", "Provider id configured under `[deploy.providers]`"),
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
            "effigy deliver deploy model --json",
            "effigy deliver deploy export acme-cloud --path infra/deploy --plan",
            "effigy deliver deploy plan uat --write-report",
            "effigy deliver deploy apply uat --yes",
            "effigy deliver deploy history production --limit 5",
            "effigy --json deliver deploy model --repo /path/to/repo",
        ],
    )
}
