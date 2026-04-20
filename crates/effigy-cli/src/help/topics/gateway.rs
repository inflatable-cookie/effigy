use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_gateway_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("gateway Help")?;
    render_info_notices(
        renderer,
        &[
            "Operate Effigy's host-native local DNS and reverse-proxy gateway.",
            "Projects that declare `[containers.<name>.dns]` now register and remove gateway routes through the container lifecycle.",
            "Use `gateway setup-tls` once per machine before enabling `tls = true` on any `[containers.<name>.dns].routes` entry.",
            "On macOS, `gateway up` and `gateway down` also manage `/etc/resolver/test` and will prompt for admin approval when host setup needs it.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy gateway up [--json]",
            "effigy gateway down [--json]",
            "effigy gateway status [--json]",
            "effigy gateway setup-tls [--json]",
            "effigy --json gateway status",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--json", "Render machine-readable gateway payloads"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy gateway up",
            "effigy gateway status --json",
            "effigy gateway setup-tls",
            "effigy gateway down",
        ],
    )?;
    Ok(())
}
