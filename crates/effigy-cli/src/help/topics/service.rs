use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_service_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("service Help")?;
    render_info_notices(
        renderer,
        &[
            "Inspect the layered service catalog used by catalog-backed container environments.",
            "Extraction writes bundled fragments into a project-local override directory so repos can take ownership without patching the bundled service catalog.",
            "Compatibility aliases: `effigy catalog ...` and `effigy catalogue ...`.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy service list [--repo <PATH>] [--json]",
            "effigy service extract <SERVICE> [--repo <PATH>] [--dir <PATH>] [--json]",
            "effigy --json service list [--repo <PATH>]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--dir <PATH>",
                "Override the extraction destination; defaults to `infra/dev/catalog` inside the repo",
            ),
            ("--json", "Render machine-readable catalog payloads"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy service list",
            "effigy service extract php-fpm",
            "effigy service extract nginx --dir infra/dev/catalog-custom",
        ],
    )?;
    Ok(())
}
