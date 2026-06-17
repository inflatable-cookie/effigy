use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_catalog_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "catalog",
        &[
            "Manage repo catalog discovery state.",
            "Clearing the discovery cache forces the next catalog walk to inspect previously pruned large empty subtrees.",
        ],
        &[
            "effigy catalog cache clear [--repo <PATH>] [--json]",
            "effigy --json catalog cache clear [--repo <PATH>]",
        ],
        &[
            ("cache clear", "Remove the repo-local catalog discovery cache"),
            ("--repo <PATH>", "Override target repository path"),
            ("--json", "Render machine-readable catalog cache payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy catalog cache clear",
            "effigy catalog cache clear --repo ~/Dev/projects/acowtancy",
        ],
    )
}
