# TTY Init Wizard Engine

Date: 2026-05-19  
Roadmap: [`g07.052`](../../../roadmaps/g07/052-tty-init-wizard-engine-and-prompt-flow.md)  
Batch card: [`1002`](../../../roadmaps/g07/batch-cards/1002-build-tty-init-wizard-engine.md)  
Strict lane: [`093`](../../../specs/093-init-setup-wizard-strict-lane.md)

## What Changed

- made plain `effigy init` TTY-aware
- added an explicit gate so the wizard path only activates when:
  - the call is the implicit default apply path
  - stdin and stdout are real TTYs
  - JSON mode is off
- preserved deterministic non-interactive behavior for:
  - `effigy init --apply`
  - `effigy init --check`
  - `effigy init --repair`
  - named starters
  - non-TTY invocation
- added a first wizard engine under the builtin init surface
- implemented two bounded prompt phases backed by the existing shipped init
  jobs:
  - baseline repo files
  - agent setup
- refactored baseline init job execution so the wizard can apply only the jobs
  selected in a phase instead of forcing all baseline setup at once
- added a concise final summary with:
  - completed actions
  - skipped phases
  - deferred actions
  - follow-up command guidance
- added focused proofs for:
  - implicit-vs-explicit init mode detection
  - TTY prompt gating
  - baseline-only phase application
  - noop wizard behavior on an already-satisfied repo

## Deliberate Limits

- the wizard engine exists before the broader adapter inventory lands
- only the baseline and agent-setup phases are currently populated with real
  runnable jobs
- later cards still need to widen the inventory into graph, tasks, bundles,
  secrets, and validation surfaces

## Validation

- `cargo test -p effigy-builtin plain_init_marks_implicit_default_apply -- --nocapture`
- `cargo test -p effigy-builtin explicit_apply_does_not_mark_implicit_default_apply -- --nocapture`
- `cargo test -p effigy-builtin wizard_can_apply_baseline_only_and_defer_agent_setup -- --nocapture`
- `cargo test -p effigy-builtin wizard_reports_repo_already_satisfied_without_prompting -- --nocapture`
- `cargo test -p effigy-builtin plain_tty_init_prompts_but_json_explicit_apply_and_non_tty_do_not -- --nocapture`
- `cargo test run_manifest_task_builtin_init_creates_scaffold_when_missing -- --nocapture`
- `cargo clippy -p effigy-builtin --tests -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`

## Next Task

Execute `1003`.
