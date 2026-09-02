use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_exec_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "exec",
        &[
            "Run one ad-hoc command inside the manifest's default system workspace container.",
            "If `--service` is omitted, Effigy targets the container's `primary_service`.",
            "Primary-service commands use the declared workspace user and HOME; piped and other non-console sessions run without a TTY.",
            "Bare alias commands still route through the same exec path when declared in `[containers.<name>.aliases]`.",
        ],
        &[
            "effigy local exec [--repo <PATH>] [--service <NAME>] [--json] <COMMAND> [ARGS...]",
            "effigy --json local exec [--repo <PATH>] [--service <NAME>] <COMMAND> [ARGS...]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--service <NAME>",
                "Target one service instead of the container primary service",
            ),
            ("--json", "Render machine-readable exec results"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy local exec composer install",
            "effigy local exec php artisan migrate",
            "effigy local exec --service db mysql",
        ],
    )
}
