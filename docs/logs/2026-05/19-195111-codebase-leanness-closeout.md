# Codebase Leanness Closeout

Date: 2026-05-19  
Roadmap: [`g07.063`](../../roadmaps/g07/063-codebase-leanness-closeout.md)  
Batch card: [`1013`](../../roadmaps/g07/batch-cards/1013-close-codebase-leanness-lane.md)  
Strict lane: [`094`](../../specs/094-codebase-leanness-and-boundary-hardening-strict-lane.md)

## What Changed

- closed the codebase leanness lane after:
  - shared graph record-emission helpers across JS, PHP, and Python
  - splitting graph query and manifest extraction into owned submodules
  - splitting init setup inventory and wizard support by concern
  - normalizing repeated help-topic and distribution JSON wrapper shapes
  - trimming one runner planning surface and one noisy fixture pattern
  - refreshing crate-boundary notes and archiving stale strict-lane specs
- landed two tiny closeout fixes needed to make broad QA honest:
  - updated stale docs references from `query.rs` and `manifest.rs` to the
    current module paths
  - rewrote the repo-local AGENTS rule to avoid the forbidden copied
    current-directory repo override literal

## Scan Delta

Baseline from `1006`:

- `effigy scan god-files --json`: `4` findings
- `effigy scan duplicate-blocks --json`: `110` findings
- `effigy scan attention-markers --json`: `0`
- `effigy scan comment-ratio --json`: `0`

Closeout rerun:

- `effigy scan god-files --json`: `3` findings, all `warning`, no `high` or
  `critical`
- `effigy scan duplicate-blocks --json`: `111` findings, `0 critical`, `7 high`
- `effigy scan attention-markers --json`: unchanged at `0`
- `effigy scan comment-ratio --json`: unchanged at `0`

Interpretation:

- the lane removed one god-file finding and cleared the remaining god-file set
  down to warning-only
- duplicate-block count did not materially improve at headline level, but the
  extractor-emission duplication that opened the lane is no longer the critical
  maintenance risk
- the lane is worth closing because the large ownership seams were reduced
  without contract drift, not because every duplicate disappeared

## Focused Validation

- `cargo fmt --all -- --check`
- `cargo test -p effigy-codegraph --quiet`
- `cargo check -p effigy-builtin --tests`
- `cargo test -p effigy-builtin inventory_detects_contextual_setup_surfaces -- --nocapture`
- `cargo test -p effigy-builtin follow_up_renderer_surfaces_real_commands -- --nocapture`
- `cargo test -p effigy-builtin wizard_prompts_for_contextual_jobs_when_baseline_is_satisfied -- --nocapture`
- `cargo test -p effigy-cli help::tests:: -- --nocapture`
- `cargo test -p effigy-distribution -- --nocapture`
- `cargo test -p effigy-docs-policy -- --nocapture`

## Broad QA

- `effigy qa`
  - `1518` tests passed, `1` skipped
  - docs link, index, JSON-example, forbidden-text, heading, workflow-path, and
    next-action checks passed
  - fast JSON contract checks passed

## Remaining Debt

Follow-up candidates:

- `crates/effigy-codegraph/src/language/manifest/semantic.rs`
- `crates/effigy-codegraph/src/tests.rs`
- `src/runner/script_command/mod.rs`

Defer:

- the remaining high duplicate-block findings in graph language extractors and
  some CLI help topics should only be reduced if a later lane can prove the
  abstraction stays readable
- runner-private temp-repo setup duplication outside the touched surfaces does
  not justify more cleanup without a dedicated runner lane

Not worth doing now:

- chasing the raw duplicate-block headline down by folding every repeated option
  row or language-specific emitter branch into a helper would trade explicit
  ownership for cosmetic scan wins

## Vision Target Delta

- primary vision tags touched: `MAINT`, `OPERATE`
- moved:
  - the graph and init surfaces are now easier to extend without carrying the
    same oversized ownership blobs
  - the lane closed on broad repo QA rather than local-only proofs
  - roadmap/spec continuation state no longer implies stale ready work
- remains open:
  - any future reduction of the remaining warning-only god files or stubborn
    duplicate clusters needs a fresh bounded lane

## Next Task

No active ready card.
