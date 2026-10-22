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
        "`release validate`, `preflight`, `proof`, `check-binary`, `verify-install` and `evidence validate|closeout|summary` are retired in v0.15: Effigy's own checks are the repo tasks `release:metadata`, `release:preflight`, `release:proof`, `release:check-binary`, `release:verify:install` and `release:evidence:validate|closeout` in `config/tasks.toml`, and `release receipt <run>` replaces the summary. A project keeps its own checks as `[tasks]` wired as `[release.gates]` or a target's `verify` (contract 054 §4.4).",
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
        "effigy release:metadata --tag v0.2.5",
        "effigy release:preflight --tag v0.2.5 --output ./artifacts/release-preflight-v0.2.5.env",
        "effigy release:check-binary ./effigy-x86_64-unknown-linux-gnu --glibc-floor 2.35",
        "effigy release:verify:install --tag v0.2.5",
        "effigy release:proof --artifacts-dir .effigy/release/artifacts",
        "effigy --json release status --check-gates",
    ],
};

const RELEASE_OPTIONS: &[(&str, &str)] = option_rows![
    "--to <STAGE>" => "Stop the release Run after this stage boundary: validate, pre-gates, prepare, certify, publish, verify or closeout (default closeout)",
    "--published" => "With `release decide`: the unknown `task` target published the candidate",
    "--not-published" => "With `release decide`: the unknown `task` target did not publish the candidate",
    "--version <SEMVER>" => "Override the changelog-derived selected version for `release --plan`",
];
