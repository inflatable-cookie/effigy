use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    option_rows, render_standard_topic_help_spec, text_lines, CommonOption, StandardTopicHelpSpec,
};

pub(crate) fn render_uninstall_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &UNINSTALL_HELP)
}

const UNINSTALL_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "uninstall",
    notices: text_lines![
        "`uninstall` plans or removes Effigy-owned local machine state.",
        "Plain `effigy uninstall` is plan-only. Mutation requires `--yes`.",
        "The first implementation removes user-global Effigy config/catalog state and the managed Colima profile; it does not remove the Effigy binary."
    ],
    usage: text_lines![
        "effigy uninstall [--json]",
        "effigy uninstall --plan [--json]",
        "effigy uninstall --yes [--json]",
    ],
    leading_common_options: &[],
    options: UNINSTALL_OPTIONS,
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable uninstall plan/result payload"),
        CommonOption::Help,
    ],
    examples: text_lines!["effigy uninstall", "effigy uninstall --yes"],
};

const UNINSTALL_OPTIONS: &[(&str, &str)] = option_rows![
    "--plan" => "Preview Effigy-owned local cleanup targets without deleting anything",
    "--yes" => "Delete planned Effigy-owned local cleanup targets without prompting",
];
