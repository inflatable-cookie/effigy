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
            "`service pack` manages independently versioned catalog packs. Installation is always explicit: an immutable `oci://...@sha256:...` reference, a local directory, or `service pack update`. Update resolves the compiled official `stable` channel to a digest, then uses the same validate-store-activate transaction. Ordinary catalog use never probes a registry.",
        ],
        &[
            "effigy local service list [--repo <PATH>] [--json]",
            "effigy local service extract <SERVICE> [--repo <PATH>] [--dir <PATH>] [--json]",
            "effigy local service pack status [--json]",
            "effigy local service pack install oci://<REPO>@sha256:<DIGEST> [--json]",
            "effigy local service pack install --path <DIR> [--json]",
            "effigy local service pack update [--json]",
            "effigy local service pack rollback [--json]",
            "effigy local service pack reset [--json]",
            "effigy --json local service list [--repo <PATH>]",
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
            "effigy local service list",
            "effigy local service extract php-fpm",
            "effigy local service extract nginx --dir infra/dev/catalog-custom",
            "effigy local service pack status",
            "effigy local service pack install --path ./catalog-pack",
            "effigy local service pack update",
            "effigy local service pack rollback",
            "effigy local service pack reset",
        ],
    )
}
