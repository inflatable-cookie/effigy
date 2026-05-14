use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_bundle_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "bundle",
        &[
            "Inspect or refresh the active repo-local `[bundle]` source.",
            "Bundle inspection reports the current source type, local materialized path, version hint, and stale state.",
            "Remote git and OCI bundle sources can be refreshed with `bundle sync`; local path bundles report not-applicable.",
        ],
        &[
            "effigy bundle inspect [--repo <PATH>] [--json]",
            "effigy bundle sync [--json]",
            "effigy --json bundle inspect",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--json", "Render machine-readable bundle payloads"),
            ("sync", "Refresh the current repo's remote git or OCI bundle source"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy bundle inspect --repo /path/to/workspace",
            "effigy bundle inspect",
            "effigy bundle sync",
            "effigy --json bundle inspect",
        ],
    )
}
