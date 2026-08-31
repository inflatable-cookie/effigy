use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_skill_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &SKILL_HELP)
}

const SKILL_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "skill",
    notices: &[
        "Load one explicit skill-owned task catalog without merging it into the consumer repository task surface.",
        "Skill tasks run on the host. The skill supplies task code; the current or --repo repository owns runtime effects.",
    ],
    usage: &[
        "effigy skill tasks --path <SKILL_DIR|EFFIGY_TOML> [--json]",
        "effigy skill run --path <SKILL_DIR|EFFIGY_TOML> <SELECTOR> [--repo <CONSUMER>] [--json] [-- <ARGS>]",
    ],
    leading_common_options: &[],
    options: &[
        ("--path <PATH>", "Required skill directory or direct effigy.toml task source"),
        ("--repo <CONSUMER>", "Consumer repository target for skill run; defaults to nearest root from invocation CWD"),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render versioned skill source/target evidence"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy skill tasks --path ~/.agents/skills/northstar",
        "effigy skill run --path ~/.agents/skills/northstar northstar/rust-quality:check",
        "effigy skill run --path /opt/skills/northstar/effigy.toml northstar/setup --repo /work/consumer -- apply",
    ],
};
