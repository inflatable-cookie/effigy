use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_gateway_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "gateway",
        &[
            "Operate Effigy's host-native local DNS and reverse-proxy gateway.",
            "Projects that declare `[containers.<name>.dns]` now register and remove gateway routes through the container lifecycle.",
            "Use `gateway setup-tls` once per machine before enabling `tls = true` on any `[containers.<name>.dns].routes` entry.",
            "When a route sets `tls = true`, plain HTTP requests now redirect to the equivalent HTTPS URL once the TLS listener is available.",
            "On macOS, `gateway up` and `gateway down` also manage `/etc/resolver/test` and will prompt for admin approval when host setup needs it.",
        ],
        &[
            "effigy gateway up [--json]",
            "effigy gateway down [--json]",
            "effigy gateway status [--json]",
            "effigy gateway setup-tls [--json]",
            "effigy --json gateway status",
        ],
        &[
            ("--json", "Render machine-readable gateway payloads"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy gateway up",
            "effigy gateway status --json",
            "effigy gateway setup-tls",
            "effigy gateway down",
        ],
    )
}
