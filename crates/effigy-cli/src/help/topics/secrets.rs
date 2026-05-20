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
            "effigy secrets list [--repo <PATH>] [--json]",
            "effigy secrets doctor [--repo <PATH>] [--json]",
            "effigy secrets init [--repo <PATH>] [--json]",
            "effigy secrets import [<PATH>] [--repo <PATH>] [--json]",
            "effigy secrets set <NAME> [--repo <PATH>] [--json]",
            "effigy secrets get <NAME> [--repo <PATH>] [--json]",
            "effigy secrets unset <NAME> [--repo <PATH>] [--json]",
            "effigy secrets change-passphrase [--repo <PATH>] [--json]",
            "effigy secrets export --format env --output <PATH> --yes [--repo <PATH>] [--json]",
            "effigy --json secrets list",
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
            "effigy secrets list",
            "effigy secrets doctor",
            "effigy secrets init",
            "effigy secrets import",
            "effigy secrets import infra/local.env",
            "effigy secrets set database_url",
            "effigy secrets get database_url",
            "effigy secrets unset database_url",
            "effigy secrets change-passphrase",
            "effigy secrets export --format env --output .effigy/runtime/secrets/local.env --yes",
            "effigy secrets list --json",
        ],
    )
}
