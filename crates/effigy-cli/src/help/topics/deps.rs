use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_deps_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &DEPS_HELP)
}

const DEPS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "deps",
    notices: &[
        "Inspect machine-local Cargo and Bun dependency links without changing committed manifests.",
        "Status is read-only. Cargo and Bun link/unlink operations are available with exact dry-run plans.",
    ],
    usage: &[
        "effigy deps [--repo <PATH>] [--json]",
        "effigy deps status [cargo|bun] [--repo <PATH>] [--json]",
        "effigy deps link <cargo|bun> <LIBRARY_PATH> [--dry-run] [--repo <PATH>] [--json]",
        "effigy deps unlink <cargo|bun> <LIBRARY_PATH> [--dry-run] [--repo <PATH>] [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        ("status [cargo|bun]", "Show desired and observed local-link state"),
        ("link cargo <PATH>", "Apply or preview a verified local Cargo patch closure"),
        ("unlink cargo <PATH>", "Remove a local Cargo patch and verify committed-source recovery"),
        ("link bun <PATH>", "Apply or preview one verified save-less Bun package closure"),
        ("unlink bun <PATH>", "Remove exact Bun links and release safe registrations"),
        ("--dry-run", "Preview link/unlink without mutation"),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render the versioned dependency status or operation payload"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy deps",
        "effigy deps status cargo",
        "effigy --json deps status bun",
        "effigy deps link cargo ../signal --dry-run",
        "effigy deps unlink cargo ../signal",
        "effigy deps link bun ../poodle --dry-run",
        "effigy deps unlink bun ../poodle",
    ],
};
