use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

pub(crate) fn render_doctor_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("doctor Help")?;
    render_info_notices(
        renderer,
        &[
            "Run remediation-first health checks for environment tooling, manifest validity, and task references.",
            "Explain task resolution with `effigy doctor <task> <args>`.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]",
            "effigy doctor <task> <args> [--json]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--fix", "Apply safe automatic remediations when available"),
            (
                "--verbose",
                "Include expanded per-finding detail in text output",
            ),
            ("--json", "Render machine-readable doctor report payload"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy doctor",
            "effigy doctor --repo /path/to/workspace",
            "effigy doctor --fix",
            "effigy doctor --verbose",
            "effigy doctor farmyard/build -- --watch",
            "effigy --json doctor --repo /path/to/workspace",
        ],
    )?;
    Ok(())
}
