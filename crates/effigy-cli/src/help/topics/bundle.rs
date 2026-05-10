use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_bundle_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "bundle",
        &[
            "Inspect the shipped top-level bundle catalog used by `[bundle]` in `effigy.toml`.",
            "Bundle inspection shows both the accepted input schema and the manifest paths the bundle injects by default.",
            "Bundle export writes the same canonical template shape that shipped bundle defaults use, but as a repo-owned local bundle directory.",
        ],
        &[
            "effigy bundle list [--repo <PATH>] [--json]",
            "effigy bundle inspect [--repo <PATH>] [<BUNDLE>] [--json]",
            "effigy bundle export [--repo <PATH>] <BUNDLE> --path <DIR> [--json]",
            "effigy bundle sync [--json]",
            "effigy --json bundle list",
            "effigy --json bundle inspect",
            "effigy --json bundle inspect decodelabs",
            "effigy --json bundle export underlay --path bundles/underlay",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--json", "Render machine-readable bundle payloads"),
            (
                "--path <DIR>",
                "Export a shipped bundle as a local bundle directory",
            ),
            ("sync", "Refresh the current repo's remote git or OCI bundle source"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy bundle list",
            "effigy bundle inspect --repo /path/to/workspace",
            "effigy bundle inspect",
            "effigy bundle inspect decodelabs",
            "effigy bundle export --repo /path/to/workspace underlay --path bundles/underlay",
            "effigy bundle export underlay --path bundles/underlay",
            "effigy bundle sync",
            "effigy --json bundle inspect decodelabs",
        ],
    )
}
