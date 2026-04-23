use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_bundle_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("bundle Help")?;
    render_info_notices(
        renderer,
        &[
            "Inspect the shipped top-level bundle catalog used by `[bundle]` in `effigy.toml`.",
            "Bundle inspection shows both the accepted input schema and the manifest paths the bundle injects by default.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy bundle list [--json]",
            "effigy bundle inspect <BUNDLE> [--json]",
            "effigy bundle export <BUNDLE> --path <DIR> [--json]",
            "effigy --json bundle list",
            "effigy --json bundle inspect decodelabs",
            "effigy --json bundle export underlay --path bundles/underlay",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--json", "Render machine-readable bundle payloads"),
            (
                "--path <DIR>",
                "Export a shipped bundle as a local bundle directory",
            ),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy bundle list",
            "effigy bundle inspect decodelabs",
            "effigy bundle export underlay --path bundles/underlay",
            "effigy --json bundle inspect decodelabs",
        ],
    )?;
    Ok(())
}
