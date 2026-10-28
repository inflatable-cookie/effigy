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
            "On Linux, setup also installs a launch-only systemd user unit, `~/.config/systemd/user/effigy-gateway.service`, enabled for this login. It runs `effigy gateway up`/`down` and nothing else, so the gateway's own kernel Run stays the lifecycle authority; `systemctl --user restart` is a reconcile request, and the unit has no restart policy.",
            "On Linux, starting the unit after boot with no logged-in user is your explicit `loginctl enable-linger \"$USER\"` opt-in; undo it with `loginctl disable-linger \"$USER\"`. Setup never enables lingering.",
            "On Linux, the administrator prompt installs the split-DNS unit and applies only `CAP_NET_BIND_SERVICE` to the installed binary; setup then proves it by binding the configured low ports as you. `gateway setup --remove` removes those, and only records Effigy wrote.",
            "A route's owner is its checkout path. The same checkout replaces its own route; another checkout takes the domain automatically when that path is gone or its target is not listening, and is refused by name otherwise.",
            "`gateway up`, `down`, `status` and route registration never request administrator access; run setup to migrate a v0.14.1 root daemon.",
            "`--private-state-root <DIR>` (or `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` for managed startup) runs a disposable, unprivileged loopback gateway with its own CA. It never touches host trust or resolver state; clients trust `<DIR>/ca/rootCA.pem` and use the reported addresses.",
        ],
        &[
            "effigy gateway setup [--suffix <DOMAIN>] [--remove] [--json]",
            "effigy gateway up [--private-state-root <DIR>] [--dns-addr <IP:PORT>] [--proxy-addr <IP:PORT>] [--https-addr <IP:PORT>] [--json]",
            "effigy gateway down [--private-state-root <DIR>] [--json]",
            "effigy gateway status [--private-state-root <DIR>] [--json]",
            "effigy gateway setup-tls [--private-state-root <DIR>] [--json]",
            "effigy --json gateway status",
        ],
        &[
            ("--suffix <DOMAIN>", "Setup only: approve an additional public resolver suffix"),
            ("--json", "Render machine-readable gateway payloads"),
            ("--private-state-root <DIR>", "Use an existing owner-only fixture state root without host trust, resolver, alias, or privilege effects"),
            ("--dns-addr <IP:PORT>", "Private mode only: choose a loopback DNS UDP bind; port 0 requests an OS-assigned port"),
            ("--proxy-addr <IP:PORT>", "Private mode only: choose a loopback HTTP bind; port 0 requests an OS-assigned port"),
            ("--https-addr <IP:PORT>", "Private mode only: choose a loopback HTTPS bind; port 0 requests an OS-assigned port"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy gateway up",
            "effigy gateway status --json",
            "effigy gateway setup",
            "effigy gateway down",
        ],
    )
}
