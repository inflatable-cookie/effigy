use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    render_bullet_section, render_info_notices, render_options_section, render_usage_section,
};

pub(crate) fn render_distribution_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    renderer.section("distribution Help")?;
    render_info_notices(
        renderer,
        &[
            "Validate release/distribution prerequisites and generate closeout evidence from artifact logs.",
            "Keep repo-specific file lists and workflow policies declarative where possible; these commands own the durable validation/reporting engine.",
        ],
    )?;
    render_usage_section(
        renderer,
        &[
            "effigy distribution validate-metadata [--repo <PATH>] [--tag <TAG>] [--json]",
            "effigy distribution check-glibc-floor [--repo <PATH>] --binary <PATH> --max-glibc <VER> [--json]",
            "effigy distribution preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]",
            "effigy distribution first-publish [--repo <PATH>] --tag <TAG> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--skip-homebrew] [--artifacts-dir <DIR>] [--json]",
            "effigy distribution validate-artifacts [--repo <PATH>] --artifacts-dir <DIR> [--expect-homebrew] [--json]",
            "effigy distribution generate-closeout [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--output <PATH>] [--owner <NAME>] [--expect-homebrew] [--json]",
            "effigy distribution write-summary [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--homebrew-executed] [--log-file <NAME>]... [--json]",
            "effigy --json distribution validate-artifacts [--repo <PATH>] --artifacts-dir <DIR>",
        ],
    )?;
    render_options_section(
        renderer,
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--tag <TAG>",
                "Release tag used for metadata validation or closeout output",
            ),
            (
                "--binary <PATH>",
                "Built binary inspected for GLIBC requirements",
            ),
            ("--max-glibc <VER>", "Maximum allowed GLIBC version"),
            (
                "--artifacts-dir <DIR>",
                "Artifact directory containing first-publish logs",
            ),
            ("--skip-docs", "Skip docs QA during distribution preflight"),
            (
                "--skip-smoke",
                "Skip distribution artifact-pipeline smoke coverage during preflight",
            ),
            (
                "--expect-homebrew",
                "Require Homebrew channel logs during artifact validation",
            ),
            (
                "--output <PATH>",
                "Override the generated closeout log path",
            ),
            (
                "--owner <NAME>",
                "Owner label written into the generated closeout log",
            ),
            (
                "--skip-homebrew",
                "Skip Homebrew install and upgrade checks",
            ),
            (
                "--log-file <NAME>",
                "Append one captured log filename to the summary contract",
            ),
            ("--json", "Render machine-readable distribution payloads"),
            ("-h, --help", "Print command help"),
        ],
    )?;
    render_bullet_section(
        renderer,
        "Examples",
        "commands",
        &[
            "effigy distribution validate-metadata --tag v0.2.5",
            "effigy distribution check-glibc-floor --binary ./effigy-x86_64-unknown-linux-gnu --max-glibc 2.35",
            "effigy distribution preflight --tag v0.2.5 --output ./artifacts/distribution-preflight-v0.2.5.env",
            "effigy distribution first-publish --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5",
            "effigy distribution validate-artifacts --artifacts-dir ./artifacts/distribution-v0.2.5",
            "effigy distribution generate-closeout --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5",
            "effigy distribution write-summary --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5 --homebrew-executed --log-file 01-tag-install-validation.log",
            "effigy distribution generate-closeout --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5 --expect-homebrew --output ./tmp/closeout.md",
        ],
    )?;
    Ok(())
}
