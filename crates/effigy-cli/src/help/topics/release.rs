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
        "`release --plan` shows the whole pipeline (contract 054 §5) with no mutation: every identity's current and next version, the files to change, gates by stage, artifacts, targets and approval checkpoints. It replaces the retired `release simulate`.",
        "`release run` runs one durable Run through validate → [pre-gates] → prepare → certify → publish → verify → closeout with a checkpoint at every boundary. `release resume <run>` continues an interrupted Run from its last checkpoint with the same candidate commit, versions and artifact digests, reconciling an unknown tag push before any retry. `release approve <run>` passes an approval checkpoint and `release receipt <run>` prints what the Run recorded.",
        "`release run` publishes every target in declared order: one annotated tag per identity, and each `task` target through its project task. A `task` target, artifact task and verify task read the certified candidate at `$EFFIGY_RELEASE_INPUT` (`effigy.release.input.v1`) and write `effigy.release.output.v1` to `$EFFIGY_RELEASE_OUTPUT` with `committed` true, false or unknown, destination IDs and an optional note.",
        "An unknown publish is never retried blind: `release resume` runs the target's `check` task first, and a target with no `check` stops at `awaiting-decision` until `release decide <run> --published|--not-published`. A gate with `reuse-hosted-evidence` is a hosted wait: the Run stops at `awaiting-hosted-evidence` without holding a process open, and `release resume` re-checks the exact-commit evidence.",
        "`release prepare`, `release execute` and the prepared-state `release resume` are retired in v0.15: use `release run`, `release resume <run>` and `release receipt`. Run state and receipts live in Effigy-owned state outside the repo.",
        "`release verify-install` is Effigy's self-hosting tagged-binary check. Library and service repositories should use a repo-owned consumer smoke instead.",
        "Gated runs persist each executed gate's full output and a redacted environment under `.effigy/reports/release/gates/`. Progress and the configured-gate inventory always go to stderr.",
    ],
    usage: text_lines![
        "effigy release status [--repo <PATH>] [--check-gates] [--json]",
        "effigy release --plan [--repo <PATH>] [--version <SEMVER>] [--json]",
        "effigy release run [--repo <PATH>] [--to <STAGE>] [--json]",
        "effigy release resume [--repo <PATH>] <RUN> [--json]",
        "effigy release approve [--repo <PATH>] <RUN> [--json]",
        "effigy release decide [--repo <PATH>] <RUN> --published|--not-published [--json]",
        "effigy release receipt [--repo <PATH>] [<RUN>] [--json]",
        "effigy release gates [--repo <PATH>] [--json]",
        "effigy release verify-install [--repo <PATH>] [--tag <TAG>] [--repo-url <URL>] [--json]",
        "effigy release validate [--repo <PATH>] [--tag <TAG>] [--json]",
        "effigy release check-binary [--repo <PATH>] <BIN> --glibc-floor <VER> [--json]",
        "effigy release preflight [--repo <PATH>] [--tag <TAG>] [--skip-docs] [--skip-smoke] [--output <PATH>] [--json]",
        "effigy release proof [--repo <PATH>] --tag <TAG> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--skip-homebrew] [--artifacts-dir <DIR>] [--json]",
        "effigy release evidence validate [--repo <PATH>] --artifacts-dir <DIR> [--expect-homebrew] [--json]",
        "effigy release evidence closeout [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--output <PATH>] [--owner <NAME>] [--expect-homebrew] [--json]",
        "effigy release evidence summary [--repo <PATH>] --tag <TAG> --artifacts-dir <DIR> [--crate-version <VER>] [--repo-url <URL>] [--brew-formula <NAME>] [--homebrew-executed] [--log-file <NAME>]... [--json]",
        "effigy --json release status [--repo <PATH>] [--check-gates]",
    ],
    leading_common_options: &[
        CommonOption::Repo,
        CommonOption::Plan,
    ],
    options: RELEASE_OPTIONS,
    trailing_common_options: &[
        CommonOption::Json("Render machine-readable release payload"),
        CommonOption::Help,
    ],
    examples: text_lines![
        "effigy release status",
        "effigy release status --repo /path/to/workspace",
        "effigy release status --check-gates",
        "effigy release gates",
        "effigy release --plan",
        "effigy release --plan --version 0.2.8",
        "effigy release run",
        "effigy release run --to certify",
        "effigy release resume 4f3c2b1a",
        "effigy release approve 4f3c2b1a",
        "effigy release decide 4f3c2b1a --published",
        "effigy release receipt",
        "effigy release receipt 4f3c2b1a --json",
        "effigy release verify-install --tag v0.2.5",
        "effigy release validate --tag v0.2.5",
        "effigy release check-binary ./effigy-x86_64-unknown-linux-gnu --glibc-floor 2.35",
        "effigy release preflight --tag v0.2.5 --output ./artifacts/distribution-preflight-v0.2.5.env",
        "effigy release proof --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5",
        "effigy release evidence validate --artifacts-dir ./artifacts/distribution-v0.2.5",
        "effigy release evidence closeout --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5",
        "effigy release evidence summary --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5 --homebrew-executed --log-file 01-tag-install-validation.log",
        "effigy --json release status --check-gates",
    ],
};

const RELEASE_OPTIONS: &[(&str, &str)] = option_rows![
    "--to <STAGE>" => "Stop the release Run after this stage boundary: validate, pre-gates, prepare, certify, publish, verify or closeout (default closeout)",
    "--published" => "With `release decide`: the unknown `task` target published the candidate",
    "--not-published" => "With `release decide`: the unknown `task` target did not publish the candidate",
    "--version <SEMVER>" => "Override the changelog-derived selected version for `release --plan`",
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
