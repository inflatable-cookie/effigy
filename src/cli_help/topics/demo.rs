use crate::ui::{Renderer, UiResult};

use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_demo_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("demo Help")?;
    renderer.text(
        "Inspect the repo-owned demo registry and latest known proof state without starting demo execution.",
    )?;
    renderer.text("")?;

    render_info_notices(
        renderer,
        &[
            "The first shipped `demo` slice is discovery and inspection only.",
            "Use `effigy demo list` to browse declared demos and `effigy demo inspect <DEMO_ID>` to inspect one record in detail.",
        ],
    )?;

    render_usage_section(
        renderer,
        &[
            "effigy demo list [--repo <PATH>] [--json]",
            "effigy demo inspect <DEMO_ID> [--repo <PATH>] [--json]",
            "effigy --json demo list [--repo <PATH>]",
            "effigy --json demo inspect <DEMO_ID> [--repo <PATH>]",
        ],
    )?;

    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Run against a different repo root"),
            (
                "--json",
                "Render machine-readable demo discovery/inspection payloads",
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
            "effigy --json demo inspect login-smoke",
        ],
    )?;

    Ok(())
}
