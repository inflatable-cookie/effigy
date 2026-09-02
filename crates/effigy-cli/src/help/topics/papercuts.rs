use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_papercuts_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &PAPERCUTS_HELP)
}

const PAPERCUTS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "papercuts",
    notices: &[
        "Discover conventional root PAPERCUTS.md queues from one project or immediate child projects.",
        "Papercuts remain observations; this command does not prioritize or promote them.",
    ],
    usage: &[
        "effigy repo papercuts [--all] [--scope <PATH>] [--json]",
        "effigy repo papercuts add <TITLE> --friction <TEXT> --impact <TEXT> --fix <TEXT> --surface <TEXT> [--scope <PATH>] [--json]",
    ],
    leading_common_options: &[],
    options: &[
        ("--scope <PATH>", "Use this project or sibling-project collection instead of the current directory"),
        ("--all", "Include closed entries in discovery output"),
        ("add <TITLE>", "Insert one canonical open entry into a single project queue"),
        ("--friction <TEXT>", "Describe what was harder than it should have been"),
        ("--impact <TEXT>", "Describe repeat cost, ambiguity, or failure mode"),
        ("--fix <TEXT>", "Describe the smallest plausible improvement"),
        ("--surface <TEXT>", "Name the affected tool, document, script, or workflow"),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render the versioned papercuts payload"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy repo papercuts",
        "effigy --json repo papercuts --scope ~/Dev/projects",
        "effigy repo papercuts --all",
        "effigy repo papercuts add \"Graph output is noisy\" --friction \"stale output floods context\" --impact \"repeat orientation cost\" --fix \"refresh once\" --surface \"Effigy graph\"",
    ],
};
