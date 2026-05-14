use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_doctor_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "doctor",
        &[
            "Run remediation-first health checks for environment tooling, manifest validity, and task references.",
            "Explain task resolution with `effigy doctor <task> <args>`.",
            "Also surfaces runtime/backend context when Docker Desktop and Colima coexist.",
        ],
        &[
            "effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]",
            "effigy doctor <task> <args> [--json]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            ("--fix", "Apply safe automatic remediations when available"),
            (
                "--verbose",
                "Include expanded per-finding detail in text output",
            ),
            ("--json", "Render machine-readable doctor report payload"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy doctor",
            "effigy doctor --repo /path/to/workspace",
            "effigy doctor --fix",
            "effigy doctor --verbose",
            "effigy doctor --verbose  # inspect Docker/Colima backend selection",
            "effigy doctor frontend/build -- --watch",
            "effigy --json doctor --repo /path/to/workspace",
        ],
    )
}
