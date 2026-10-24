use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_gateway_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "gateway",
        &[
            "Operate Effigy's local DNS and reverse-proxy gateway for `*.test` routes.",
            "Containers with `[containers.<name>.dns]` register and remove their routes as they start and stop.",
            "Run `gateway setup` once to install resolver, loopback and TLS trust prerequisites; the daemon runs as your user.",
            "`gateway repair` finds route-table conflicts and dead owners: loopback assignments, container routes (alias and HTTP) and withdrawn domain claims whose checkout, containers, runtime scope and listeners are all gone.",
            "`gateway repair` previews what it would release and its plan digest; `--yes` applies it and `--yes --expect-plan <DIGEST>` refuses when the reviewed plan changed. Released addresses and domains stay in the endpoint reuse quarantine for about three minutes, so live owners never change address.",
            "`--owner-checkout <PATH>` narrows preview and apply to the one deleted checkout you name, so recovering a stale claim never releases another project's endpoints. The digest covers the filter: applying it to a different owner refuses.",
            "`gateway up`, `down`, `status` and route registration never request administrator access; run setup to migrate a v0.14.1 root daemon.",
            "`--private-state-root <DIR>` (or `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` for managed startup) runs a disposable, unprivileged loopback gateway with its own CA. It never touches host trust or resolver state; clients trust `<DIR>/ca/rootCA.pem` and use the reported addresses.",
        ],
        &[
            "effigy gateway setup [--suffix <DOMAIN>] [--remove] [--json]",
            "effigy gateway up [--private-state-root <DIR>] [--dns-addr <IP:PORT>] [--proxy-addr <IP:PORT>] [--https-addr <IP:PORT>] [--json]",
            "effigy gateway down [--private-state-root <DIR>] [--json]",
            "effigy gateway status [--private-state-root <DIR>] [--json]",
            "effigy gateway repair [--owner-checkout <PATH>] [--yes [--expect-plan <DIGEST>]] [--json]",
            "effigy gateway setup-tls [--private-state-root <DIR>] [--json]",
            "effigy --json gateway status",
        ],
        &[
            ("--suffix <DOMAIN>", "Setup only: approve an additional public resolver suffix"),
            ("--json", "Render machine-readable gateway payloads"),
            ("--yes", "Apply repairable route-table cleanup and release dead loopback owners"),
            ("--expect-plan <DIGEST>", "Repair only: the dead-owner plan digest you reviewed. Apply refuses if the recomputed plan differs"),
            ("--owner-checkout <PATH>", "Repair only: narrow the dead-owner plan to this exact deleted checkout path"),
            ("--private-state-root <DIR>", "Use an existing owner-only fixture state root without host trust, resolver, alias, or privilege effects"),
            ("--dns-addr <IP:PORT>", "Private mode only: choose a loopback DNS UDP bind; port 0 requests an OS-assigned port"),
            ("--proxy-addr <IP:PORT>", "Private mode only: choose a loopback HTTP bind; port 0 requests an OS-assigned port"),
            ("--https-addr <IP:PORT>", "Private mode only: choose a loopback HTTPS bind; port 0 requests an OS-assigned port"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy gateway up",
            "effigy gateway status --json",
            "effigy gateway repair",
            "effigy gateway repair --yes",
            "effigy gateway repair --yes --expect-plan <DIGEST>",
            "effigy gateway repair --owner-checkout /Users/me/worktrees/old-checkout",
            "effigy gateway repair --owner-checkout /Users/me/worktrees/old-checkout --yes --expect-plan <DIGEST>",
            "effigy gateway setup",
            "effigy gateway down",
        ],
    )
}
