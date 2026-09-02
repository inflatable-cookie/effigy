use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_deps_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &DEPS_HELP)
}

const DEPS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "deps",
    notices: &[
        "Inspect machine-local Cargo and Bun links, or author committed Bun overrides explicitly.",
        "Link state is machine-local and save-less. Bun pin state is committed and requires a separate bun install.",
    ],
    usage: &[
        "effigy admin deps [--repo <PATH>] [--json]",
        "effigy admin deps status [cargo|bun] [--repo <PATH>] [--json]",
        "effigy admin deps link <cargo|bun> <LIBRARY_PATH> [--dry-run] [--repo <PATH>] [--json]",
        "effigy admin deps unlink <cargo|bun> <LIBRARY_PATH> [--dry-run] [--repo <PATH>] [--json]",
        "effigy admin deps pin bun <LIBRARY_PATH> [--dry-run] [--repo <PATH>] [--json]",
        "effigy admin deps unpin bun <LIBRARY_PATH> [--dry-run] [--repo <PATH>] [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("status [cargo|bun]", "Show desired and observed local-link state"),
        ("link cargo <PATH>", "Apply or preview a verified local Cargo patch closure"),
        ("unlink cargo <PATH>", "Remove a local Cargo patch and verify committed-source recovery"),
        ("link bun <PATH>", "Apply or preview one verified save-less Bun package closure"),
        ("unlink bun <PATH>", "Remove exact Bun links and release safe registrations"),
        ("pin bun <PATH>", "Add one committed full-closure Bun override plan"),
        ("unpin bun <PATH>", "Remove only exact committed Bun override matches"),
        ("--dry-run", "Preview dependency mutation without writing"),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render the versioned dependency status or operation payload"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy admin deps",
        "effigy admin deps status cargo",
        "effigy --json admin deps status bun",
        "effigy admin deps link cargo ../signal --dry-run",
        "effigy admin deps unlink cargo ../signal",
        "effigy admin deps link bun ../poodle --dry-run",
        "effigy admin deps unlink bun ../poodle",
        "effigy admin deps pin bun ../poodle --dry-run",
        "effigy admin deps unpin bun ../poodle",
    ],
};
