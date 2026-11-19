use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_state_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "state",
        &[
            "Inspect, stage, and capture standalone data artifacts without moving app semantics into Effigy.",
            "`state artifact` stages local files and explicit `oci://` refs and reports the immutable digest of what it staged; OCI transport uses the local `oras` CLI plus its normal registry auth config.",
            "The v0.16 retirement of the layered state stack (`state plan|apply|capture|history`) and what a repository declares instead are in `docs/knowledge/deployment-data-workflows.md`.",
        ],
        &[
            "effigy state artifact inspect <REF|PATH> [--repo <PATH>] [--farmyard-handoff] [--json]",
            "effigy state artifact stage <REF|PATH> [--repo <PATH>] [--farmyard-handoff] [--json]",
            "effigy state artifact capture <SOURCE_PATH|DIR> --ref oci://<REF> [--kind <KIND>] [--environment <LABEL>] [--farmyard-handoff] [--push] [--json]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--ref oci://<REF>",
                "Record the planned OCI destination for `state artifact capture`",
            ),
            (
                "--environment <LABEL>",
                "Attach an environment label such as `uat` to a captured data artifact",
            ),
            (
                "--farmyard-handoff",
                "Include a stable Farmyard handoff block for app-local migration tools",
            ),
            (
                "--kind <KIND>",
                "Override a captured data-artifact kind such as `sql-dump` or `content-overlay`",
            ),
            (
                "--push",
                "Publish the capture to its explicit OCI ref after local staging",
            ),
            ("--json", "Render machine-readable artifact payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy state artifact inspect seed.sql --json",
            "effigy state artifact stage ./data/legacy.sql.gz --farmyard-handoff --json",
            "effigy state artifact capture ./state/media --ref oci://ghcr.io/acme/media:uat --kind object-store --push --json",
            "effigy state artifact capture ./dumps/uat.sql.gz --ref oci://ghcr.io/acme/uat-content:2026-05-06 --environment uat --json",
            "effigy state artifact inspect oci://ghcr.io/acme/private-data:uat",
        ],
    )
}
