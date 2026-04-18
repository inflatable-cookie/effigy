use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_exec_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("exec Help")?;
    render_info_notices(
        renderer,
        &[
            "Run one ad-hoc command inside the manifest's `context = \"dev\"` container.",
            "If `--service` is omitted, Effigy targets the container's `primary_service`.",
            "Bare alias commands still route through the same exec path when declared in `[containers.<name>.exec.aliases]`.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy exec [--repo <PATH>] [--service <NAME>] [--json] <COMMAND> [ARGS...]",
            "effigy --json exec [--repo <PATH>] [--service <NAME>] <COMMAND> [ARGS...]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--service <NAME>",
                "Target one service instead of the container primary service",
            ),
            ("--json", "Render machine-readable exec results"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy exec composer install",
            "effigy exec php artisan migrate",
            "effigy exec --service db mysql",
        ],
    )?;
    Ok(())
}
