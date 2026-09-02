use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_secrets_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "secrets",
        &[
            "Inspect and manage the local Effigy secrets vault.",
            "Mutation commands store values in `[secrets.vault].path` and never print values.",
        ],
        &[
            "effigy admin secrets list [--repo <PATH>] [--json]",
            "effigy admin secrets doctor [--repo <PATH>] [--json]",
            "effigy admin secrets init [--repo <PATH>] [--json]",
            "effigy admin secrets import [<PATH>] [--repo <PATH>] [--json]",
            "effigy admin secrets set <NAME> [--repo <PATH>] [--json]",
            "effigy admin secrets get <NAME> [--repo <PATH>] [--json]",
            "effigy admin secrets unset <NAME> [--repo <PATH>] [--json]",
            "effigy admin secrets change-passphrase [--repo <PATH>] [--json]",
            "effigy admin secrets export --format env --output <PATH> --yes [--repo <PATH>] [--json]",
            "effigy --json admin secrets list",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--json", "Render machine-readable secrets payloads"),
            (
                "list",
                "List declared secret names, targets, and safe metadata",
            ),
            (
                "doctor",
                "Check declaration/backend config without reading values",
            ),
            ("init", "Create an empty encrypted local vault"),
            (
                "import [<PATH>]",
                "Import declared secrets from a .env-style file; defaults to ./.env",
            ),
            ("set <NAME>", "Set a declared secret value"),
            ("get <NAME>", "Print one declared secret value"),
            ("unset <NAME>", "Remove a declared secret value"),
            (
                "change-passphrase",
                "Re-encrypt the vault with a new passphrase",
            ),
            (
                "export",
                "Write a confirmed plaintext compatibility file without printing values",
            ),
            ("--format env", "Export dotenv-compatible KEY=VALUE lines"),
            ("--output <PATH>", "Write export to a file, never stdout"),
            ("--yes", "Required confirmation for plaintext export"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy admin secrets list",
            "effigy admin secrets doctor",
            "effigy admin secrets init",
            "effigy admin secrets import",
            "effigy admin secrets import infra/local.env",
            "effigy admin secrets set database_url",
            "effigy admin secrets get database_url",
            "effigy admin secrets unset database_url",
            "effigy admin secrets change-passphrase",
            "effigy admin secrets export --format env --output .effigy/runtime/secrets/local.env --yes",
            "effigy admin secrets list --json",
        ],
    )
}
