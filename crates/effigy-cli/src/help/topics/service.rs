use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_service_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "service",
        &[
            "Inspect the layered service catalog used by catalog-backed container environments.",
            "Extraction writes bundled fragments into a project-local override directory so repos can take ownership without patching the bundled service catalog.",
            "Catalog layers resolve in order: project override, user override, active installed pack, compiled baseline. The compiled baseline always ships with Effigy, so nothing here needs a pack store, `oras`, or a network.",
            "`service pack` manages independently versioned catalog packs. Installation is always explicit: an immutable `oci://...@sha256:...` reference or a local directory. A candidate is validated before it is activated, a failed candidate leaves the previous selection alone, and an installed pack that later becomes unreadable falls back visibly to the compiled baseline.",
        ],
        &[
            "effigy service list [--repo <PATH>] [--json]",
            "effigy service extract <SERVICE> [--repo <PATH>] [--dir <PATH>] [--json]",
            "effigy service pack status [--json]",
            "effigy service pack install oci://<REPO>@sha256:<DIGEST> [--json]",
            "effigy service pack install --path <DIR> [--json]",
            "effigy service pack rollback [--json]",
            "effigy service pack reset [--json]",
            "effigy --json service list [--repo <PATH>]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--dir <PATH>",
                "Override the extraction destination; defaults to `infra/dev/catalog` inside the repo",
            ),
            (
                "--path <DIR>",
                "Install a catalog pack from an explicitly selected local directory",
            ),
            ("--json", "Render machine-readable catalog payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy service list",
            "effigy service extract php-fpm",
            "effigy service extract nginx --dir infra/dev/catalog-custom",
            "effigy service pack status",
            "effigy service pack install --path ./catalog-pack",
            "effigy service pack rollback",
            "effigy service pack reset",
        ],
    )
}
