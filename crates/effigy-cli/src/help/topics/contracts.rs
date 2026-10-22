use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_contracts_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &CONTRACTS_HELP)
}

const CONTRACTS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "contracts",
    notices: &[
        "Validate the project's own JSON artifacts with one Draft 2020-12 JSON Schema engine instead of jq-heavy shell scripts.",
        "The schema index and the selection contract come from `[contracts]` in effigy.toml; there is no built-in default path.",
    ],
    usage: &[
        "effigy contracts check-json [--repo <PATH>] [--index <PATH>] [--contract <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected[=text|json]] [--validate-selection] [--artifact <PATH>] [--json]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        (
            "--index <PATH>",
            "Override `contracts.schema_index` from effigy.toml",
        ),
        (
            "--contract <PATH>",
            "Override `contracts.selection_contract` from effigy.toml",
        ),
        ("--fast", "Run the lighter JSON contract subset"),
        ("--full", "Run the full active JSON contract set"),
        (
            "--changed-only <BASE>",
            "Restrict active rows to entries changed relative to a git base ref",
        ),
        (
            "--print-selected",
            "Print selected schema ids as text before running checks",
        ),
        (
            "--print-selected=json",
            "Print the selected-schema payload as a single JSON line before running checks; also validates it against the selection contract",
        ),
        (
            "--validate-selection",
            "Validate the selection payload against the selection contract",
        ),
        (
            "--artifact <PATH>",
            "Validate the selection payload stored in this artifact file instead of this run's payload",
        ),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable validation payloads"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy contracts check-json --fast --print-selected",
        "effigy contracts check-json --fast --changed-only origin/main --print-selected=json",
        "effigy contracts check-json --validate-selection",
        "effigy contracts check-json --validate-selection --artifact tmp/selected.json",
        "effigy --json contracts check-json --print-selected=json --validate-selection",
    ],
};
