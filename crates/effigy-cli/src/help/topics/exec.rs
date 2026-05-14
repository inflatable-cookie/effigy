use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_exec_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "exec",
        &[
            "Run one ad-hoc command inside the manifest's `context = \"dev\"` container.",
            "If `--service` is omitted, Effigy targets the container's `primary_service`.",
            "Bare alias commands still route through the same exec path when declared in `[containers.<name>.aliases]`.",
        ],
        &[
            "effigy exec [--repo <PATH>] [--service <NAME>] [--json] <COMMAND> [ARGS...]",
            "effigy --json exec [--repo <PATH>] [--service <NAME>] <COMMAND> [ARGS...]",
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
            "effigy exec composer install",
            "effigy exec php artisan migrate",
            "effigy exec --service db mysql",
        ],
    )
}
