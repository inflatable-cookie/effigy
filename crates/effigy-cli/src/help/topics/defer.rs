use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_defer_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &DEFER_HELP)
}

const DEFER_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "defer",
    notices: &[
        "Run the configured `[defer]` fallback explicitly instead of relying on selector miss routing.",
        "Use this when you want the configured `[defer]` behavior even though the requested task name is known up front.",
        "Container deferral reuses the normal container/runtime path and short-circuits to local execution when already inside an Effigy handoff container.",
    ],
    usage: &[
        "effigy defer <REQUEST> [args...]",
        "effigy defer [--repo <PATH>] [--json] <REQUEST> [args...]",
        "effigy --json defer <REQUEST> [args...]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable command-envelope JSON"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy defer prep",
        "effigy defer release -- --dry-run",
        "effigy defer --repo /path/to/legacy-site seed",
        "effigy --json defer prep",
    ],
};
