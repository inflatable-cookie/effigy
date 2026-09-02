use super::super::{HelpRenderer, HelpResult};
use super::shared::{render_standard_topic_help_spec, CommonOption, StandardTopicHelpSpec};

pub(crate) fn render_contracts_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &CONTRACTS_HELP)
}

const CONTRACTS_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "contracts",
    notices: &[
        "Validate reusable JSON contract artifacts from Effigy-owned command surfaces instead of jq-heavy shell scripts.",
        "Keep contract structure in JSON files and repo-specific policy in task wiring; the built-in command just enforces the published shape.",
    ],
    usage: &[
        "effigy repo contracts check-json [--repo <PATH>] [--index <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]",
        "effigy repo contracts validate-selection [--repo <PATH>] [--contract <PATH>] [--artifact <PATH>] [--json]",
        "effigy --json repo contracts validate-selection [--repo <PATH>] [--artifact <PATH>]",
    ],
    leading_common_options: &[CommonOption::Repo],
    options: &[
        (
            "--index <PATH>",
            "Override the JSON schema index file (defaults to `docs/contracts/json-schema-index.json`)",
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
            "Print the selected-schema payload as a single JSON line before running checks",
        ),
        (
            "--contract <PATH>",
            "Override the JSON contract file (defaults to `docs/contracts/json-selection-contract.json`)",
        ),
        (
            "--artifact <PATH>",
            "Override the artifact file (defaults to `json-contracts-selected.json`)",
        ),
    ],
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable validation payloads"),
        CommonOption::Help,
    ],
    examples: &[
        "effigy repo contracts check-json --fast --print-selected",
        "effigy repo contracts check-json --fast --changed-only origin/main --print-selected=json",
        "effigy repo contracts validate-selection",
        "effigy repo contracts validate-selection --artifact tmp/selected.json",
        "effigy --json repo contracts validate-selection --artifact json-contracts-selected.json",
    ],
};
