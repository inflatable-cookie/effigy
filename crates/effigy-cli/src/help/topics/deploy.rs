use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_deploy_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "deploy",
        &[
            "Inspect the provider-neutral production deployment model derived from the effective manifest and bundle state.",
            "The first shipped surface is JSON-only and currently supports Underlay repos only.",
        ],
        &[
            "effigy deploy model [--repo <PATH>] --json",
            "effigy --json deploy model [--repo <PATH>]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--json", "Render the deployment model payload"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy deploy model --json",
            "effigy --json deploy model --repo /path/to/repo",
        ],
    )
}
