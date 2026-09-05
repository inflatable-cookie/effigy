# g08.009 - Code Quality Boundary Sweep Suite

Status: Complete
Depends on: `g08.008`
Completed: 2026-06-04

## Goal

Turn the 2026-06-04 code quality sweep into a bounded follow-up tranche that
reduces drift-prone declarations, separates mixed system decisions, and cuts
safe duplication without changing public behavior.

## Scope

- make command and Rhai helper declarations harder to desynchronize
- split container bring-up into clearer planning, execution, integration, and
  rendering phases
- centralize stable repo-marker and manifest-filename definitions
- continue low-risk duplicate-block reduction where ownership is clear
- improve test fixture builders where repeated setup hides the behavior under
  test
- tune graph-aware scan usage so dead-code and boundary findings become more
  actionable in this repo

## Guardrails

- no CLI grammar changes
- no JSON schema id or schema-version changes
- no help text redesign
- no release execution
- no `.github/workflows/` edits
- no feature removal unless a later card proves the feature is unused and asks
  for human confirmation
- no speculative macros or generated command framework

## Batch Slices

- [`1037-open-code-quality-boundary-sweep-lane.md`](./batch-cards/1037-open-code-quality-boundary-sweep-lane.md):
  opened the code-quality boundary sweep lane and recorded baseline evidence.
- [`1038-define-command-surface-descriptor-seam.md`](./batch-cards/1038-define-command-surface-descriptor-seam.md):
  completed command-surface descriptor convergence.
- [`1039-define-rhai-feature-descriptor-seam.md`](./batch-cards/1039-define-rhai-feature-descriptor-seam.md):
  completed Rhai feature descriptor convergence.
- [`1040-split-container-up-phase-helpers.md`](./batch-cards/1040-split-container-up-phase-helpers.md):
  completed container `up` phase boundary cleanup.
- [`1041-converge-repo-marker-rules.md`](./batch-cards/1041-converge-repo-marker-rules.md):
  completed Effigy repo-marker and root-rule convergence.
- [`1042-reduce-selected-duplicate-blocks.md`](./batch-cards/1042-reduce-selected-duplicate-blocks.md):
  completed selected duplicate-block follow-through.
- [`1043-tune-boundary-and-dead-code-scans-for-effigy.md`](./batch-cards/1043-tune-boundary-and-dead-code-scans-for-effigy.md):
  completed boundary/dead-code scan self-adoption.
- [`1044-fix-dead-code-scan-rust-signal.md`](./batch-cards/1044-fix-dead-code-scan-rust-signal.md):
  fixed dead-code Rust signal quality after self-adoption showed the scanner
  was too noisy to use without graph/indexer changes.
- [`1045-classify-and-reduce-dead-code-residuals.md`](./batch-cards/1045-classify-and-reduce-dead-code-residuals.md):
  completed Rust `#[test]` entrypoint handling while keeping unused test
  helpers visible.
- [`1046-classify-trait-and-api-surface-dead-code.md`](./batch-cards/1046-classify-trait-and-api-surface-dead-code.md):
  completed trait/API surface method handling while keeping private inherent
  methods visible.
- [`1047-classify-descriptor-and-dispatch-dead-code-roots.md`](./batch-cards/1047-classify-descriptor-and-dispatch-dead-code-roots.md):
  completed descriptor/dispatch root handling for function-pointer-owned
  helpers.
- [`1048-classify-dto-render-config-dead-code-roots.md`](./batch-cards/1048-classify-dto-render-config-dead-code-roots.md):
  completed DTO/render/config data-shape root handling.
- [`1049-classify-rust-impl-and-associated-call-dead-code.md`](./batch-cards/1049-classify-rust-impl-and-associated-call-dead-code.md):
  completed Rust impl and associated-call precision handling.
- [`1050-complete-dead-code-false-positive-burn-down.md`](./batch-cards/1050-complete-dead-code-false-positive-burn-down.md):
  completed final false-positive handling and deleted confirmed dead artifacts.

## Evidence From Sweep

- `effigy scan god-files --json`: 0 findings
- `effigy scan comment-ratio --json`: 0 findings
- `effigy scan attention-markers --json`: 0 findings
- `effigy scan duplicate-blocks --json`: 114 findings, 2 high
- `effigy scan boundary-violations --json`: no configured layers
- `effigy scan dead-code --json`: noisy advisory output, not deletion-ready
- `effigy test --plan`: selected `cargo nextest run`
- `effigy doctor`: one existing fixture-schema error in
  `tests/fixtures/graph-agent-benchmark/split-owner/effigy.toml`

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`023-container-command-decomposition-contract.md`](../../contracts/023-container-command-decomposition-contract.md)
- [`024-shared-dispatcher-and-exec-collapse-contract.md`](../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md)
- [`028-manifest-section-decomposition-contract.md`](../../contracts/028-manifest-section-decomposition-contract.md)
- [`030-low-risk-deduplication-contract.md`](../../contracts/030-low-risk-deduplication-contract.md)
- [`031-artifact-and-crate-boundary-contract.md`](../../contracts/031-artifact-and-crate-boundary-contract.md)

## Acceptance Criteria

- the first ready card opens the lane with baseline evidence and no code change
- every implementation card preserves public command behavior
- duplicate-block reductions are tied to clear ownership boundaries
- graph-aware scans are either configured for Effigy or documented as advisory
- closeout records what was reduced, what stayed deliberately explicit, and
  which findings remain deferred

## Evidence

- [`../../logs/archive/2026-06/04-204300-code-quality-boundary-sweep-lane-opened.md`](../../logs/archive/2026-06/04-204300-code-quality-boundary-sweep-lane-opened.md)
- [`../../logs/archive/2026-06/04-210225-command-surface-descriptor-seam.md`](../../logs/archive/2026-06/04-210225-command-surface-descriptor-seam.md)
- [`../../logs/archive/2026-06/04-210845-rhai-feature-descriptor-seam.md`](../../logs/archive/2026-06/04-210845-rhai-feature-descriptor-seam.md)
- [`../../logs/archive/2026-06/04-212126-container-up-phase-boundary-cleanup.md`](../../logs/archive/2026-06/04-212126-container-up-phase-boundary-cleanup.md)
- [`../../logs/archive/2026-06/04-214009-repo-marker-root-rule-convergence.md`](../../logs/archive/2026-06/04-214009-repo-marker-root-rule-convergence.md)
- [`../../logs/archive/2026-06/04-214831-selected-duplicate-block-follow-through.md`](../../logs/archive/2026-06/04-214831-selected-duplicate-block-follow-through.md)
- [`../../logs/archive/2026-06/04-215614-boundary-dead-code-self-adoption.md`](../../logs/archive/2026-06/04-215614-boundary-dead-code-self-adoption.md)
- [`../../logs/archive/2026-06/04-221542-dead-code-scan-rust-signal-correction.md`](../../logs/archive/2026-06/04-221542-dead-code-scan-rust-signal-correction.md)
- [`../../logs/archive/2026-06/04-223151-dead-code-test-scope-filter.md`](../../logs/archive/2026-06/04-223151-dead-code-test-scope-filter.md)
- [`../../logs/archive/2026-06/04-225805-dead-code-trait-surface-precision.md`](../../logs/archive/2026-06/04-225805-dead-code-trait-surface-precision.md)
- [`../../logs/archive/2026-06/04-230651-dead-code-descriptor-root-precision.md`](../../logs/archive/2026-06/04-230651-dead-code-descriptor-root-precision.md)
- [`../../logs/archive/2026-06/04-231646-dead-code-data-shape-root-precision.md`](../../logs/archive/2026-06/04-231646-dead-code-data-shape-root-precision.md)
- [`../../logs/archive/2026-06/04-232355-dead-code-rust-impl-call-precision.md`](../../logs/archive/2026-06/04-232355-dead-code-rust-impl-call-precision.md)
- [`../../logs/archive/2026-06/04-233355-dead-code-final-burn-down.md`](../../logs/archive/2026-06/04-233355-dead-code-final-burn-down.md)

## Current Evidence Baseline

After `1050`, `target/debug/effigy scan dead-code --json` reports:

- findings: 0
- isolated files: 0
- unreferenced symbols: 0

The current Effigy dead-code scan has no remaining findings.

## Next Task

No active ready card remains for this sweep.
