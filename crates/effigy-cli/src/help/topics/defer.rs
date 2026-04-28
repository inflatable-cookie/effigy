use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_defer_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "defer",
        &[
            "Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing.",
            "Use this when you want the configured `[defer]` behavior even though the requested task name is known up front.",
            "Container deferral reuses the normal container/runtime path and short-circuits to local execution when already inside an Effigy handoff container.",
        ],
        &[
            "effigy defer <REQUEST> [args...]",
            "effigy defer [--repo <PATH>] [--json] <REQUEST> [args...]",
            "effigy --json defer <REQUEST> [args...]",
        ],
        &[
            ("--repo <PATH>", "Run against a different repo root"),
            ("--json", "Render machine-readable command-envelope JSON"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy defer prep",
            "effigy defer release -- --dry-run",
            "effigy defer --repo /path/to/legacy-site seed",
            "effigy --json defer prep",
        ],
    )
}
