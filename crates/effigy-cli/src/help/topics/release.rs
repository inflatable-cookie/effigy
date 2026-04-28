use super::super::{HelpRenderer, HelpResult};
use super::shared::render_standard_topic_help;

pub(crate) fn render_release_help<R: HelpRenderer>(renderer: &mut R) -> HelpResult<()> {
    render_standard_topic_help(
        renderer,
        "release",
        &[
            "Inspect release readiness from changelog state, version files, and configured gates.",
            "Text-mode release preparation and execution now use compact review menus by default; interactive prepare can jump between version review, mutation inspection, gate results, and final approval, while interactive execute can jump between stale warnings, prepared-state review, working-tree inspection, and final approval. Those menus now keep a compact command legend plus the current selected version or stale-acknowledgement state visible while you review, mark which sections were already reviewed, and blocked prepare/execute output now adds suggested remediation actions instead of only raw blocker lines. `effigy release resume` is the dedicated prepared-state recovery entrypoint: it summarizes `.release-prepared.json`, highlights drift since prepare time, and can hand off directly into execute review. Prepared release state now records source fingerprints, so `resume` and `execute` can detect branch drift, HEAD movement, and prepared-file content drift instead of relying only on raw working-tree presence checks. Those recovery menus now also expose direct `gates`, `reprepare`, and `discard` shortcuts so operators can inspect gates, regenerate prepared state, or clear stale state without leaving the interactive flow. `--plan` stays non-destructive, `--dry-run` aliases that preview mode, and `--yes` stays the explicit non-interactive path.",
        ],
        &[
            "effigy release status [--repo <PATH>] [--check-gates] [--json]",
            "effigy release gates [--repo <PATH>] [--json]",
            "effigy release resume [--repo <PATH>] [--allow-stale] [--json]",
            "effigy release verify-install [--repo <PATH>] [--tag <TAG>] [--repo-url <URL>] [--json]",
            "effigy release simulate [--repo <PATH>] [--version <SEMVER>] [--json]",
            "effigy release prepare [--repo <PATH>] [--check-gates]",
            "effigy release prepare (--plan|--dry-run) [--repo <PATH>] [--check-gates] [--version <SEMVER>] [--json]",
            "effigy release prepare --yes [--repo <PATH>] [--check-gates] [--version <SEMVER>] [--json]",
            "effigy release execute [--repo <PATH>] [--allow-stale]",
            "effigy release execute (--plan|--dry-run) [--repo <PATH>] [--allow-stale] [--json]",
            "effigy release execute --yes [--repo <PATH>] [--allow-stale] [--json]",
            "effigy --json release status [--repo <PATH>] [--check-gates]",
        ],
        &[
            ("--repo <PATH>", "Override target repository path"),
            (
                "--plan",
                "Preview release preparation or execution checks without prompting or irreversible actions",
            ),
            (
                "--dry-run",
                "Alias for `--plan` on `release prepare` and `release execute` preview flows",
            ),
            (
                "--yes",
                "Apply prepared release changes or execute commit/tag/push without interactive confirmation",
            ),
            (
                "--check-gates",
                "Run configured release gate commands before reporting readiness (interactive prepare auto-checks configured gates by default)",
            ),
            (
                "--version <SEMVER>",
                "Override the changelog-derived selected version for `release simulate`, `release prepare --plan`, or `release prepare --yes`",
            ),
            (
                "--allow-stale",
                "Acknowledge a stale `.release-prepared.json` state and allow `release execute` to continue",
            ),
            (
                "--tag <TAG>",
                "Release tag used for install verification (falls back to `GITHUB_REF_NAME` when omitted)",
            ),
            (
                "--repo-url <URL>",
                "Git repository URL used for tag install verification",
            ),
            ("--json", "Render machine-readable release status payload"),
            ("-h, --help", "Print command help"),
        ],
        &[
            "effigy release status",
            "effigy release status --repo /path/to/workspace",
            "effigy release status --check-gates",
            "effigy release gates",
            "effigy release resume",
            "effigy release resume --allow-stale",
            "effigy release verify-install --tag v0.2.5",
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
    )
}
