use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_service_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "service",
        &[
            "Inspect the layered service catalog used by catalog-backed container environments.",
            "Extraction writes bundled fragments into a project-local override directory so repos can take ownership without patching the bundled service catalog.",
        ],
        &[
            "effigy service list [--repo <PATH>] [--json]",
            "effigy service extract <SERVICE> [--repo <PATH>] [--dir <PATH>] [--json]",
            "effigy --json service list [--repo <PATH>]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--dir <PATH>",
                "Override the extraction destination; defaults to `infra/dev/catalog` inside the repo",
            ),
            ("--json", "Render machine-readable catalog payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy service list",
            "effigy service extract php-fpm",
            "effigy service extract nginx --dir infra/dev/catalog-custom",
        ],
    )
}
