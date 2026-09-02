use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_bundle_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &BUNDLE_HELP)
}

const BUNDLE_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "bundle",
    notices: &[
        "Inspect or refresh the active repo-local `[bundle]` source.",
        "Bundle inspection reports the current source type, local materialized path, version hint, and stale state.",
        "Remote git and OCI bundle sources can be refreshed with `bundle sync`; local path bundles report not-applicable.",
    ],
    usage: &[
        "effigy bundle inspect [--repo <PATH>] [--json]",
        "effigy bundle sync [--json]",
        "effigy --json bundle inspect",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[(
        "sync",
        "Refresh the current repo's remote git or OCI bundle source",
    )],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable bundle payloads"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy bundle inspect --repo /path/to/workspace",
        "effigy bundle inspect",
        "effigy bundle sync",
        "effigy --json bundle inspect",
    ],
};
