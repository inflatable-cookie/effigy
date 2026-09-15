use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_drafts_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &DRAFTS_HELP)
}

const DRAFTS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "drafts",
    notices: &[
        "Drafts are lifecycle-labelled provisional definitions under `[drafts]`. They never appear in `effigy tasks`, help, or completion, and expiry is advisory: an expired draft stays runnable until a human removes or deliberately extends it.",
    ],
    usage: &["effigy drafts [FILTER] [--repo <PATH>] [--json] [--pretty true|false]"],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        (
            "FILTER",
            "Limit the inventory to one draft selector, optionally `<catalog>/<draft>`",
        ),
        (
            "--pretty <true|false>",
            "When used with --json, toggle pretty formatting (default: true)",
        ),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render the versioned `effigy.drafts.v1` payload"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy drafts",
        "effigy drafts provider-smoke",
        "effigy drafts --json",
        "effigy drafts --repo /path/to/workspace",
    ],
};
