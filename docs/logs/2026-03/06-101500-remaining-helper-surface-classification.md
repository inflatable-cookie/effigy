# Remaining Helper Surface Classification

Status: complete
Created: 2026-03-06
Roadmap: g01.015
Batch: 15.4-helper-surface-classification

## Summary

Locked the post-self-hosting classification for remaining non-Effigy helper
entrypoints in the Effigy repo.

Excluded from this inventory:
- `src/bin/effigy.rs` because it is the primary product entrypoint, not a helper
  surface.

Classification totals:
- migrate into product code/tests: 8
- keep as thin external wrapper: 13
- defer pending explicit script-system decision: 9

## Changes

- inventoried current helper entrypoints across:
  - `.cargo/config.toml`
  - `src/bin/*.rs` helper bins
  - `scripts/*.sh`
  - `docs/scripts/*.sh`
- classified each remaining helper surface after Effigy self-hosting landed
- locked explicit multi-surface exceptions so future cleanup batches can remove
  them intentionally instead of by drift

## Decision Matrix

### 1) Migrate Into Product Code/Tests

These surfaces now overlap a canonical Effigy-first command path and should be
treated as migration targets rather than long-term primary interfaces.

- `.cargo/config.toml`
  - rationale: Cargo aliases are compatibility muscle-memory only now that
    `effigy qa:*` tasks exist.
- `src/bin/effigy-qa.rs`
  - rationale: thin Rust launcher for `scripts/check-quality-gates.sh`; target
    state is Effigy-owned QA orchestration rather than a separate helper bin.
- `src/bin/effigy-release-qa.rs`
  - rationale: thin Rust launcher for `scripts/check-release-gates.sh`; same
    migration logic as `effigy-qa`.
- `scripts/check-quality-gates.sh`
  - rationale: internal QA aggregator now fronted by `effigy qa:*`.
- `scripts/check-release-gates.sh`
  - rationale: internal release QA aggregator now fronted by
    `effigy qa:release`.
- `scripts/check-json-contracts.sh`
  - rationale: internal validation path with no standalone operator value once
    Effigy-first QA orchestration is fully settled.
- `scripts/check-json-contracts-ci.sh`
  - rationale: CI-specialized sibling to `check-json-contracts.sh`; same target
    migration path.
- `scripts/check-prepush-ci.sh`
  - rationale: contributor workflow helper that should eventually collapse into
    Effigy task composition plus targeted tests.

### 2) Keep As Thin External Wrapper

These surfaces cross a boundary where standalone executables/scripts remain
practical: local shell bootstrap, release packaging, artifact validation, or
external workflow integration.

- `scripts/effigy-dev`
  - rationale: shell wrapper is the correct abstraction for source-tree dev
    execution and symlink-based local command setup.
- `scripts/install-local-bin-links.sh`
  - rationale: filesystem symlink management is appropriately shell-owned.
- `scripts/check-release-install-from-tag.sh`
  - rationale: standalone release/tag install validation remains useful for
    external runbooks and CI.
- `scripts/check-release-smoke.sh`
  - rationale: release-artifact smoke target remains a low-level binary check.
- `scripts/check-distribution-first-publish.sh`
  - rationale: first-publish flow spans artifact layout and release environment
    concerns outside current Effigy runtime scope.
- `scripts/check-distribution-preflight.sh`
  - rationale: distribution preflight emits file-based outputs for workflow
    consumption.
- `scripts/check-distribution-metadata.sh`
  - rationale: release metadata validation is workflow-facing and artifact-driven.
- `scripts/check-distribution-artifact-pipeline-smoke.sh`
  - rationale: pipeline smoke validation is external-contract oriented.
- `scripts/validate-distribution-artifacts.sh`
  - rationale: artifact validator should remain callable as a standalone
    executable contract.
- `scripts/generate-distribution-closeout-log.sh`
  - rationale: release closeout generation remains a file-producing workflow
    utility.
- `scripts/update-homebrew-formula-from-metadata.sh`
  - rationale: Homebrew metadata/formula update stays at the packaging boundary.
- `scripts/validate-json-contract-selection-artifact.sh`
  - rationale: artifact-shape validator is intentionally standalone for CI use.
- `scripts/check-selection-artifact-validator-smoke.sh`
  - rationale: validator smoke check remains paired with the standalone artifact
    validator.

### 3) Defer Pending Explicit Script-System Decision

These surfaces are internal docs-governance or docs-QA utilities. They are real
cleanup candidates, but Effigy should not absorb them into product code until a
deliberate decision exists for repo-policy scripting.

- `docs/scripts/check-doc-workflow-paths.sh`
- `docs/scripts/check-vision-index.sh`
- `docs/scripts/check-vision-metadata.sh`
- `docs/scripts/check-vision-next-task-regression.sh`
- `docs/scripts/check-vision-next-task.sh`
- `scripts/add-log-index-entry.sh`
- `scripts/check-doc-json-examples.sh`
- `scripts/check-doc-links.sh`
- `scripts/check-doc-logs-index.sh`

Shared rationale:
- these are repo-specific governance helpers rather than end-user product
  behavior
- they have limited value as public Effigy command surfaces
- they should move only after a later decision selects either Rust-based docs
  tooling, a retained shell policy, or another explicit first-party script
  system

## Multi-Surface Exceptions

Retained exceptions after Batch 15.4:

- QA aggregation currently exists in four layers:
  - `effigy` tasks (`qa`, `qa:docs`, `qa:json`, `qa:json:ci`, `qa:release`)
  - Cargo aliases in `.cargo/config.toml`
  - Rust helper bins (`effigy-qa`, `effigy-release-qa`)
  - shell aggregators (`check-quality-gates.sh`, `check-release-gates.sh`)
  - decision: keep temporarily, but treat Cargo aliases, helper bins, and shell
    aggregators as migration surfaces rather than canonical interfaces
- local command bootstrap exists in three layers:
  - stable installed `effigy`
  - `effigy-dev`
  - direct `cargo run --bin effigy -- ...`
  - decision: keep this intentionally; it is the local channel contract, not
    accidental duplication
- docs QA exists in layered composition:
  - `effigy qa:docs`
  - `scripts/check-quality-gates.sh --docs-only`
  - `docs/scripts/*` and `scripts/check-doc-*.sh`
  - decision: keep for now and revisit only in a dedicated docs-policy tooling
    batch

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`, `RELEASE`
- Movement: baseline `self-hosting existed but remaining helper surfaces were not
  formally classified` -> current `every remaining helper entrypoint is assigned
  to migration, thin-wrapper retention, or deferred script-policy scope`
- Remaining gap: `implement the chosen migrations and publish the AI-agent /
  cross-repo adoption contract in Batch 15.5`

## Validation Performed

- command: `find scripts docs/scripts src/bin .cargo -maxdepth 2 -type f | sort`
  - result: pass; current helper surface inventory captured
- command: `sed -n '1,220p' .cargo/config.toml src/bin/effigy-qa.rs src/bin/effigy-release-qa.rs`
  - result: pass; Cargo alias and helper-bin delegation surfaces confirmed
- command: `sed -n '1,260p' docs/logs/2026-03/10-090000-script-surface-unification-batch-1.md`
  - result: pass; prior batch-1 decisions loaded as classification baseline
- command: `./scripts/check-doc-links.sh docs/logs/README.md docs/logs/2026-03/06-101500-remaining-helper-surface-classification.md docs/roadmaps/g01/015-effigy-self-hosting-and-agent-first-adoption.md`
  - result: pass; new log and roadmap links resolve
- command: `./scripts/check-doc-logs-index.sh`
  - result: pass; new log entry is indexed in `docs/logs/README.md`

## Risks

- There is still temporary overlap between Effigy tasks, Cargo aliases, helper
  bins, and shell scripts in QA/release paths; partial cleanup without a locked
  matrix would create drift.
- Docs-governance scripts are easy to keep indefinitely by inertia; they need a
  future explicit decision rather than quiet neglect.
- External release/distribution wrappers should not be migrated piecemeal into
  product code without first preserving their standalone workflow contracts.

## Next Task

Execute Batch 15.5 by publishing the AI-agent and cross-repo adoption contract,
then use this classification matrix to decide which migration surfaces can be
retired in the first follow-on cleanup batch.
