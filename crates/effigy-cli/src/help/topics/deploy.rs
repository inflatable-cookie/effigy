use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_deploy_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "deploy",
        &[
            "Inspect the provider-neutral production deployment model derived from the effective manifest and bundle state.",
            "The first shipped surfaces stay Underlay-first: the neutral model is JSON-only, and the first provider export targets are Render and Railway.",
        ],
        &[
            "effigy deploy model [--repo <PATH>] --json",
            "effigy deploy export render [--repo <PATH>] --path <DIR> [--plan] [--json]",
            "effigy deploy export railway [--repo <PATH>] --path <DIR> [--plan] [--json]",
            "effigy --json deploy model [--repo <PATH>]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--path <DIR>", "Write export files under this directory"),
            ("--plan", "Preview the export without writing files"),
            ("--json", "Render machine-facing model or export payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy deploy model --json",
            "effigy deploy export render --path infra/render --plan",
            "effigy deploy export railway --path infra/railway --plan",
            "effigy --json deploy model --repo /path/to/repo",
        ],
    )
}
