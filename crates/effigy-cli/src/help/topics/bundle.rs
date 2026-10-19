use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_bundle_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &BUNDLE_HELP)
}

const BUNDLE_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "bundle",
    notices: &[
        "Inspect pinned bundle bytes or explicitly sync checkout sources.",
        "Bundle inspection reports the current source type, local materialized path, version hint, and stale state.",
        "Only `bundle sync` updates path, git and OCI pins, including declared catalog sources; inspection never fetches or writes.",
    ],
    usage: &[
        "effigy bundle inspect [--repo <PATH>] [--json]",
        "effigy bundle sync [--json]",
        "effigy --json bundle inspect",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[(
        "sync",
        "Acquire and atomically select the checkout's bundle and catalog source pins",
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
