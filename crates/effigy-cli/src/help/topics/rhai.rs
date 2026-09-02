use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_rhai_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &RHAI_HELP)
}

const RHAI_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "rhai",
    notices: &[
        "Inspect Effigy's in-process Rhai host API surface.",
        "Use this before writing scripts when you need module names, function names, and side-effect posture without leaving the runtime.",
    ],
    usage: &["effigy extend rhai surface [--json]"],
    leading_common_options: &[],
    options: &[("surface", "List registered Rhai modules and functions")],
    trailing_common_options: &[
        CommonOption::Json("Render the Rhai surface as a machine-readable payload"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy extend rhai surface",
        "effigy extend rhai surface --json",
        "effigy --json extend rhai surface",
    ],
};
