use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_draft_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(
        renderer,
        &StandardTopicHelpSpec {
            topic: "draft",
            notices: &[
                r#"`effigy draft <task>` is a deprecated compatibility spelling. Run `effigy <task>` directly; internal tasks use the same task runtime and identity as public tasks."#,
            ],
            usage: &["effigy tasks [--all] [--json]", "effigy <task> [task args]"],
            leading_common_options: &[],
            options: &[],
            trailing_common_options: &[CommonOption::Help],
            examples: &["effigy tasks --all", "effigy --plan <task>"],
        },
    )
}
