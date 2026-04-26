use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_container_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("container Help")?;
    render_info_notices(
        renderer,
        &[
            "Operate one manifest-defined Colima-backed container environment by name or through the manifest default alias.",
            "V1 stays explicit: host-facing ports and repo-relative mounts are declared in `[containers.*.host]`, and attached sessions shut the environment down on owner exit by default.",
            "Generated compose also supports bounded `shared = true` backing services for standalone shared databases and caches on the product-owned path.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy container up [--repo <PATH>] [--attach|--detach] [--json]",
            "effigy container <NAME> up [--repo <PATH>] [--attach|--detach] [--json]",
            "effigy container down [--repo <PATH>] [--json]",
            "effigy container down --all [--json]",
            "effigy container <NAME> down [--repo <PATH>] [--json]",
            "effigy container status [--repo <PATH>] [--json]",
            "effigy container status --all [--json]",
            "effigy container stats --all [--json]",
            "effigy container <NAME> status [--repo <PATH>] [--json]",
            "effigy container data list [--repo <PATH>] [--json]",
            "effigy container <NAME> data list [--repo <PATH>] [--json]",
            "effigy container data export <VOLUME> <PATH> [--repo <PATH>] [--json]",
            "effigy container <NAME> data export <VOLUME> <PATH> [--repo <PATH>] [--json]",
            "effigy container data import <VOLUME> <PATH> [--repo <PATH>] [--json]",
            "effigy container <NAME> data import <VOLUME> <PATH> [--repo <PATH>] [--json]",
            "effigy container data pull-production [--repo <PATH>] [--json]",
            "effigy container <NAME> data pull-production [--repo <PATH>] [--json]",
            "effigy container <NAME> logs [--repo <PATH>] [--service <NAME>] [--follow] [--json]",
            "effigy container <NAME> shell [--repo <PATH>] [--service <NAME>] [--command <CMD>]",
            "effigy container <NAME> reset [--repo <PATH>] [--keep-data] [--json]",
            "effigy container <NAME> eject [--repo <PATH>] [--json]",
            "effigy --json container up [--repo <PATH>]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--attach",
                "Force attached owner-session behavior for `up` even if the manifest defaults to detached startup",
            ),
            (
                "--detach",
                "Force non-attached bring-up for `up` and exit once the environment reaches ready state",
            ),
            (
                "--service <NAME>",
                "Select one explicit service for `logs` or `shell` instead of the manifest `primary_service`",
            ),
            (
                "--all",
                "For `down`, `status`, or `stats`, discover running Effigy-managed environments across repos instead of one manifest target",
            ),
            (
                "--command <CMD>",
                "Run one shell command string inside the selected service via `sh -lc`",
            ),
            (
                "--follow",
                "Keep streaming container logs instead of returning one bounded snapshot",
            ),
            (
                "--keep-data",
                "For `reset`, preserve generated-compose persistent named volumes and remove only ephemeral ones",
            ),
            ("--json", "Render machine-readable container payloads"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy container up",
            "effigy container web up --detach",
            "effigy container web status",
            "effigy container down --all",
            "effigy container status --all",
            "effigy container stats --all",
            "effigy container web data list",
            "effigy container web data export fixture-web-dev-db-data ./backup.tar.gz",
            "effigy container web data import fixture-web-dev-db-data ./backup.tar.gz",
            "effigy container web data pull-production",
            "effigy container web logs --follow",
            "effigy container web shell --command \"php artisan tinker\"",
            "effigy container web reset --keep-data",
            "effigy container web reset",
            "effigy container web eject",
        ],
    )?;
    Ok(())
}
