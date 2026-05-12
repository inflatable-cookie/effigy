use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_secrets_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
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
            "effigy secrets set <NAME> [--repo <PATH>] [--json]",
            "effigy secrets unset <NAME> [--repo <PATH>] [--json]",
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
            ("set <NAME>", "Set a declared secret value"),
            ("unset <NAME>", "Remove a declared secret value"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy secrets list",
            "effigy secrets doctor",
            "effigy secrets init",
            "effigy secrets set database_url",
            "effigy secrets unset database_url",
            "effigy secrets list --json",
        ],
    )
}
