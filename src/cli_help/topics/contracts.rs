use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};
use crate::ui::{Renderer, UiResult};

pub(crate) fn render_contracts_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("contracts Help")?;
    render_info_notices(
        renderer,
        &[
            "Validate reusable JSON contract artifacts from Effigy-owned command surfaces instead of jq-heavy shell scripts.",
            "Keep contract structure in JSON files and repo-specific policy in task wiring; the built-in command just enforces the published shape.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy contracts check-json [--repo <PATH>] [--index <PATH>] [--fast|--full] [--changed-only <BASE>] [--print-selected|--print-selected=json] [--json]",
            "effigy contracts validate-selection [--repo <PATH>] [--contract <PATH>] [--artifact <PATH>] [--json]",
            "effigy --json contracts validate-selection [--repo <PATH>] [--artifact <PATH>]",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
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
            ("--json", "Render machine-readable validation payloads"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy contracts check-json --fast --print-selected",
            "effigy contracts check-json --fast --changed-only origin/main --print-selected=json",
            "effigy contracts validate-selection",
            "effigy contracts validate-selection --artifact tmp/selected.json",
            "effigy --json contracts validate-selection --artifact json-contracts-selected.json",
        ],
    )?;
    Ok(())
}
