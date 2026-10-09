use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_gateway_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "gateway",
        &[
            "Operate Effigy's host-native local DNS and reverse-proxy gateway.",
            "Projects that declare `[containers.<name>.dns]` now register and remove gateway routes through the container lifecycle.",
            "Use `gateway setup-tls` once per machine before enabling `tls = true` on any `[containers.<name>.dns].routes` entry.",
            "When a route sets `tls = true`, plain HTTP requests now redirect to the equivalent HTTPS URL once the TLS listener is available.",
            "`gateway repair` inspects local route-table drift such as duplicate TCP bind tuples and can remove stale conflicting container routes with `--yes`.",
            "`gateway recover` restores an ordinary upgrade past a live pre-identity daemon: confirmed-absence cleanup, or `--adopt-candidate` with interactive digest consent then a generation-bound stop. `--yes` cannot adopt a live candidate.",
            "On macOS, `gateway up` and `gateway down` also manage `/etc/resolver/test` and will prompt for admin approval when host setup needs it.",
            "For disposable consumers, `--private-state-root <DIR>` selects caller-owned state and client-scoped TLS trust; private mode binds only loopback, never installs trust or writes host resolver/alias state, and reports the actual listener addresses.",
            "Managed container startup can select the same mode with `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>`; clients must explicitly trust `<DIR>/ca/rootCA.pem` and resolve route names to the reported HTTPS loopback address.",
        ],
        &[
            "effigy gateway up [--private-state-root <DIR>] [--dns-addr <IP:PORT>] [--proxy-addr <IP:PORT>] [--https-addr <IP:PORT>] [--json]",
            "effigy gateway down [--private-state-root <DIR>] [--json]",
            "effigy gateway status [--private-state-root <DIR>] [--json]",
            "effigy gateway recover [--yes] [--adopt-candidate] [--json]",
            "effigy gateway repair [--yes] [--json]",
            "effigy gateway setup-tls [--private-state-root <DIR>] [--json]",
            "effigy --json gateway status",
        ],
        &[
            ("--json", "Render machine-readable gateway payloads"),
            ("--yes", "Apply repairable route-table cleanup, or skip the recover prompt only when no live legacy daemon remains"),
            ("--adopt-candidate", "Inspect a live legacy daemon and request a generation-bound stop after typing its candidate digest"),
            ("--private-state-root <DIR>", "Use an existing owner-only fixture state root without host trust, resolver, alias, or privilege effects"),
            ("--dns-addr <IP:PORT>", "Private mode only: choose a loopback DNS UDP bind; port 0 requests an OS-assigned port"),
            ("--proxy-addr <IP:PORT>", "Private mode only: choose a loopback HTTP bind; port 0 requests an OS-assigned port"),
            ("--https-addr <IP:PORT>", "Private mode only: choose a loopback HTTPS bind; port 0 requests an OS-assigned port"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy gateway up",
            "effigy gateway status --json",
            "effigy gateway recover --adopt-candidate",
            "effigy gateway repair",
            "effigy gateway repair --yes",
            "effigy gateway setup-tls",
            "effigy gateway down",
        ],
    )
}
