use super::super::{HelpRenderer, HelpResult};
use super::shared::{
    option_rows, render_standard_topic_help_spec, text_lines, CommonOption, StandardTopicHelpSpec,
};

pub(crate) fn render_release_help<R: HelpRenderer + ?Sized>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help_spec(renderer, &RELEASE_HELP)
}

const RELEASE_HELP: StandardTopicHelpSpec = StandardTopicHelpSpec {
    topic: "release",
    notices: text_lines![
        "Inspect release readiness from changelog state, version files, and configured gates.",
        "`release verify-install` is Effigy's self-hosting tagged-binary check. Library and service repositories should use a repo-owned consumer smoke instead.",
        "Gated runs persist each executed gate's full output and a redacted environment under `.effigy/reports/release/gates/`. Prepare and execute text shows a failed gate's last 20 lines plus its log path. Progress and the configured-gate inventory always go to stderr.",
        "Text-mode release preparation and execution now use compact review menus by default; interactive prepare can jump between version review, mutation inspection, gate results, and final approval, while interactive execute can jump between stale warnings, prepared-state review, working-tree inspection, and final approval. Those menus now keep a compact command legend plus the current selected version or stale-acknowledgement state visible while you review, mark which sections were already reviewed, and blocked prepare/execute output now adds suggested remediation actions instead of only raw blocker lines. `effigy release resume` is the dedicated prepared-state recovery entrypoint: it summarizes `.release-prepared.json`, highlights drift since prepare time, and can hand off directly into execute review. Prepared release state now records source fingerprints, so `resume` and `execute` can detect branch drift, HEAD movement, and prepared-file content drift instead of relying only on raw working-tree presence checks. Those recovery menus now also expose direct `gates`, `reprepare`, and `discard` shortcuts so operators can inspect gates, regenerate prepared state, or clear stale state without leaving the interactive flow. `--plan` stays non-destructive, `--dry-run` aliases that preview mode, and `--yes` stays the explicit non-interactive path.",
    ],
    usage: text_lines![
        "effigy release status [--repo <PATH>] [--check-gates] [--json]",
        "effigy release gates [--repo <PATH>] [--json]",
        "effigy release resume [--repo <PATH>] [--allow-stale] [--json]",
        "effigy release verify-install [--repo <PATH>] [--tag <TAG>] [--repo-url <URL>] [--json]",
        "effigy release validate [--repo <PATH>] [--tag <TAG>] [--json]",
        "effigy release check-binary [--repo <PATH>] <BIN> --glibc-floor <VER> [--json]",
        "effigy release preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]",
        "effigy release proof [--repo <PATH>] --tag <TAG> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--skip-homebrew] [--artifacts-dir <DIR>] [--json]",
        "effigy release evidence validate [--repo <PATH>] --artifacts-dir <DIR> [--expect-homebrew] [--json]",
        "effigy release evidence closeout [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--output <PATH>] [--owner <NAME>] [--expect-homebrew] [--json]",
        "effigy release evidence summary [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--homebrew-executed] [--log-file <NAME>]... [--json]",
        "effigy release simulate [--repo <PATH>] [--version <SEMVER>] [--json]",
        "effigy release prepare [--repo <PATH>] [--check-gates]",
        "effigy release prepare (--plan|--dry-run) [--repo <PATH>] [--check-gates] [--version <SEMVER>] [--json]",
        "effigy release prepare --yes [--repo <PATH>] [--check-gates] [--version <SEMVER>] [--json]",
        "effigy release execute [--repo <PATH>] [--allow-stale]",
        "effigy release execute (--plan|--dry-run) [--repo <PATH>] [--allow-stale] [--json]",
        "effigy release execute --yes [--repo <PATH>] [--allow-stale] [--json]",
        "effigy --json release status [--repo <PATH>] [--check-gates]",
    ],
    leading_common_options: &[
        CommonOption::Repo,
        CommonOption::Plan,
        CommonOption::DryRun,
        CommonOption::Yes(
            "Apply prepared release changes or execute commit/tag/push without interactive confirmation",
        ),
        CommonOption::CheckGates,
    ],
    options: RELEASE_OPTIONS,
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable release status payload"),
        CommonOption::Help,
    ],
    examples: text_lines![
        "effigy release status",
        "effigy release status --repo /path/to/workspace",
        "effigy release status --check-gates",
        "effigy release gates",
        "effigy release resume",
        "effigy release resume --allow-stale",
        "effigy release verify-install --tag v0.2.5",
        "effigy release validate --tag v0.2.5",
        "effigy release check-binary ./effigy-x86_64-unknown-linux-gnu --glibc-floor 2.35",
        "effigy release preflight --tag v0.2.5 --output ./artifacts/distribution-preflight-v0.2.5.env",
        "effigy release proof --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5",
        "effigy release evidence validate --artifacts-dir ./artifacts/distribution-v0.2.5",
        "effigy release evidence closeout --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5",
        "effigy release evidence summary --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5 --homebrew-executed --log-file 01-tag-install-validation.log",
        "effigy release simulate",
        "effigy release simulate --version 0.2.8",
        "effigy release prepare",
        "effigy release prepare --dry-run --version 0.2.8",
        "effigy release prepare --plan --version 0.2.8",
        "effigy release prepare --yes --check-gates --version 0.2.8",
        "effigy release execute",
        "effigy release execute --allow-stale",
        "effigy release execute --dry-run",
        "effigy release execute --plan",
        "effigy release execute --yes",
        "effigy --json release prepare --plan --check-gates",
        "effigy --json release status --check-gates",
    ],
};

const RELEASE_OPTIONS: &[(&str, &str)] = option_rows![
    "--version <SEMVER>" => "Override the changelog-derived selected version for `release simulate`, `release prepare --plan`, or `release prepare --yes`",
    "--allow-stale" => "Acknowledge age-based `.release-prepared.json` staleness; source drift still requires `release prepare`",
    "--tag <TAG>" => "Effigy release tag used for binary install verification (falls back to `GITHUB_REF_NAME` when omitted)",
    "--repo-url <URL>" => "Effigy Git repository URL used for binary tag install verification",
    "--glibc-floor <VER>" => "Maximum allowed GLIBC version for Linux release binaries",
    "--artifacts-dir <DIR>" => "Artifact directory containing release proof logs",
    "--skip-docs" => "Skip docs QA during release preflight",
    "--skip-smoke" => "Skip artifact-pipeline smoke coverage during release preflight",
    "--skip-homebrew" => "Skip Homebrew install and upgrade checks during release proof",
    "--expect-homebrew" => "Require Homebrew channel logs during evidence validation",
    "--output <PATH>" => "Override generated preflight or closeout output path",
    "--owner <NAME>" => "Owner label written into generated closeout evidence",
    "--log-file <NAME>" => "Append one captured log filename to the evidence summary contract",
    "--homebrew-executed" => "Mark Homebrew channel evidence as captured in the summary contract",
];
