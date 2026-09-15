use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_draft_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &DRAFT_HELP)
}

const DRAFT_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "draft",
    notices: &[
        "`effigy draft` selects only `[drafts]`; it reuses catalog alias, cwd-nearest, and shallowest-unambiguous routing, then runs through the ordinary task request and pipeline. Ordinary flat invocation never falls through to a draft.",
    ],
    usage: &[
        "effigy draft <SELECTOR> [--repo <PATH>] [--json] [-- <ARGS>]",
        "effigy draft <catalog>/<SELECTOR> [-- <ARGS>]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        (
            "<SELECTOR>",
            "Draft to run; use `<catalog>/<draft>` to pin the catalog",
        ),
        (
            "-- <ARGS>",
            "Pass remaining arguments to the draft through the normal pipeline",
        ),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render the ordinary task-run JSON result, naming the draft surface"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy draft provider-smoke",
        "effigy draft provider-smoke -- --verbose",
        "effigy draft provider-smoke --json",
        "effigy --json draft provider-smoke",
    ],
};
