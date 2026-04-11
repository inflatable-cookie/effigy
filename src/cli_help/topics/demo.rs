use crate::ui::{Renderer, UiResult};

use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_demo_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("demo Help")?;
    renderer.text("Discover, inspect, execute, and control the repo-owned demo registry.")?;
    renderer.text("")?;

    render_info_notices(
        renderer,
        &[
            "Use `effigy demo list` to browse declared demos, `effigy demo inspect <DEMO_ID>` to inspect one record in detail, `effigy demo run <DEMO_ID>` to record a new normalized attempt, and `stop` or `rerun` when lifecycle control exists for that demo.",
        ],
    )?;

    render_usage_section(
        renderer,
        &[
            "effigy demo list [--repo <PATH>] [--json]",
            "effigy demo inspect <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy demo run <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy demo stop <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy demo rerun <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy --json demo list [--repo <PATH>]",
            "effigy --json demo inspect <DEMO_ID> [--repo <PATH>]",
            "effigy --json demo run <DEMO_ID> [--repo <PATH>]",
            "effigy --json demo stop <DEMO_ID> [--repo <PATH>]",
            "effigy --json demo rerun <DEMO_ID> [--repo <PATH>]",
        ],
    )?;

    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Run against a different repo root"),
            (
                "--json",
                "Render machine-readable demo discovery, inspection, or run payloads",
            ),
            ("-h, --help", "Print demo command help"),
        ],
    )?;

    render_bullet_section(
        renderer,
        "Examples",
        "example",
        &[
            "effigy demo list",
            "effigy demo inspect plugin-capability-browser",
            "effigy demo run login-smoke",
            "effigy demo stop login-smoke",
            "effigy demo rerun login-smoke",
            "effigy --json demo inspect login-smoke",
        ],
    )?;

    Ok(())
}
