use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_service_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "service",
        &[
            "Inspect the service catalog used by catalog-backed container environments.",
            "Catalog sources resolve in order: the project catalog (`infra/dev/catalog/`), the bundle's shipped `catalog/`, declared `[catalog.sources.<name>]` sources, then the compiled baseline. `service list` names the source of each service and what it shadows.",
            "Select one source with a qualified name, `<source>/<service>`, for example `catalog = \"baseline/redis\"`. Source handles are `project`, `bundle`, each declared name, and `baseline`.",
            "Declared sources are git, OCI or path sources pinned per checkout, like bundles. Only `effigy bundle sync` on the host moves their pins; reads never fetch.",
            "Extraction copies a baseline fragment into the project catalog so a repo can take ownership of it.",
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
            "effigy bundle sync",
        ],
    )
}
