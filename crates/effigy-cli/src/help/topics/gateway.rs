use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_gateway_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "gateway",
        &[
            "Operate Effigy's local DNS and reverse-proxy gateway for `*.test` routes.",
            "Containers with `[containers.<name>.dns]` register and remove their routes as they start and stop.",
            "Run `gateway setup-tls` once per machine before using `tls = true` routes; plain HTTP then redirects to HTTPS.",
            "`gateway repair` finds route-table conflicts and removes stale container routes with `--yes`.",
            "`gateway recover` replaces a gateway left running by an older release; `--adopt-candidate` stops a live one after you confirm its digest.",
            "On macOS, `gateway up` and `gateway down` manage `/etc/resolver/test` and ask for admin approval when needed.",
            "`--private-state-root <DIR>` (or `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` for managed startup) runs a disposable, unprivileged loopback gateway with its own CA. It never touches host trust or resolver state; clients trust `<DIR>/ca/rootCA.pem` and use the reported addresses.",
            "Experimental, not for general use: `setup-listener-handoff`, `listener-handoff-status` and `remove-listener-handoff` print or inspect a launchd definition for a user-owned gateway. They install, run and remove nothing (guide 063).",
        ],
        &[
            "effigy gateway up [--private-state-root <DIR>] [--dns-addr <IP:PORT>] [--proxy-addr <IP:PORT>] [--https-addr <IP:PORT>] [--json]",
            "effigy gateway down [--private-state-root <DIR>] [--json]",
            "effigy gateway status [--private-state-root <DIR>] [--json]",
            "effigy gateway recover [--yes] [--adopt-candidate] [--json]",
            "effigy gateway repair [--yes] [--json]",
            "effigy gateway setup-tls [--private-state-root <DIR>] [--json]",
            "effigy gateway setup-listener-handoff [--json]",
            "effigy gateway listener-handoff-status [--json]",
            "effigy gateway remove-listener-handoff [--json]",
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
            "effigy gateway listener-handoff-status --json",
            "effigy gateway remove-listener-handoff",
            "effigy gateway down",
        ],
    )
}
