use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_drafts_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(
        renderer,
        &StandardTopicHelpSpec {
            topic: "drafts",
            notices: &[
                r#"`effigy drafts` was removed in v0.15. Use `effigy tasks --all` or `effigy tasks --json`. Legacy `[drafts]` tables translate to internal tasks for one release; move each definition to `[tasks.<name>]` with `visibility = "internal"` and remove lifecycle metadata."#,
            ],
            usage: &["effigy tasks [--all] [--json]", "effigy <task> [task args]"],
            leading_common_options: &[],
            options: &[],
            trailing_common_options: &[CommonOption::Help],
            examples: &["effigy tasks --all", "effigy --plan <task>"],
        },
    )
}
