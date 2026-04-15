use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

pub(crate) fn render_container_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("container Help")?;
    render_info_notices(
        renderer,
        &[
            "Operate one manifest-defined Colima-backed container environment by name or through the manifest default alias.",
            "V1 stays explicit: host-facing ports and repo-relative mounts are declared in `[containers.*.host]`, and attached sessions shut the environment down on owner exit by default.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy container up [--repo <PATH>] [--attach|--detach] [--json]",
            "effigy container <NAME> up [--repo <PATH>] [--attach|--detach] [--json]",
            "effigy container down [--repo <PATH>] [--json]",
            "effigy container <NAME> down [--repo <PATH>] [--json]",
            "effigy container status [--repo <PATH>] [--json]",
            "effigy container <NAME> status [--repo <PATH>] [--json]",
            "effigy container <NAME> logs [--repo <PATH>] [--service <NAME>] [--follow] [--json]",
            "effigy container <NAME> shell [--repo <PATH>] [--service <NAME>] [--command <CMD>]",
            "effigy container <NAME> reset [--repo <PATH>] [--json]",
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
                "--command <CMD>",
                "Run one shell command string inside the selected service via `sh -lc`",
            ),
            (
                "--follow",
                "Keep streaming container logs instead of returning one bounded snapshot",
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
            "effigy container web logs --follow",
            "effigy container web shell --command \"php artisan tinker\"",
            "effigy container web reset",
        ],
    )?;
    Ok(())
}
