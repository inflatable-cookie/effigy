use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_artifact_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "artifact",
        &[
            "Inspect and stage standalone data artifacts for seed, apply, and capture workflows.",
            "Local files and directories are staged into the repo-owned Effigy artifact cache. OCI references are explicit with `oci://` and use the local `oras` CLI plus its normal registry auth config.",
        ],
        &[
            "effigy deliver artifact inspect <REF|PATH> [--repo <PATH>] [--farmyard-handoff] [--json]",
            "effigy deliver artifact stage <REF|PATH> [--repo <PATH>] [--farmyard-handoff] [--json]",
            "effigy deliver artifact capture <SOURCE_PATH|DIR> --ref oci://<REF> [--kind <KIND>] [--environment <LABEL>] [--farmyard-handoff] [--push] [--json]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--ref oci://<REF>", "Record the planned OCI destination for `capture`"),
            (
                "--kind <KIND>",
                "Override captured artifact kind such as `sql-dump` or `content-overlay`",
            ),
            (
                "--environment <LABEL>",
                "Attach an environment label such as `uat` to captured metadata",
            ),
            (
                "--farmyard-handoff",
                "Include a stable Farmyard handoff block for app-local migration tools",
            ),
            (
                "--push",
                "Publish a captured artifact to the explicit OCI ref instead of returning a planned capture only",
            ),
            ("--json", "Render machine-readable artifact payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy deliver artifact inspect seed.sql --json",
            "effigy deliver artifact stage ./data/legacy.sql.gz --farmyard-handoff --json",
            "effigy deliver artifact capture ./state/media --ref oci://ghcr.io/acme/media:uat --kind object-store --push --json",
            "effigy deliver artifact capture ./dumps/uat.sql.gz --ref oci://ghcr.io/acme/uat-content:2026-05-06 --environment uat --json",
            "effigy deliver artifact capture ./dumps/uat.sql.gz --ref oci://ghcr.io/acme/uat-content:2026-05-06 --push --json",
            "effigy deliver artifact inspect oci://ghcr.io/acme/private-data:uat",
        ],
    )
}
