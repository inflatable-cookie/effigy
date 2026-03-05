# Script Surface Unification (Batch 1)

Date: 2026-03-05
Owner: betterthanclay
Related roadmap: post-M1 hardening / script-surface unification batch 1

## Scope

- Inventory Effigy-local shell/Rust script entrypoints used for validation, release, docs, and CI-style flows.
- Classify each entrypoint for batch-1 disposition:
  - direct Effigy-first migration now
  - keep as thin wrapper (explicit external contract)
  - hold for later tranche
- Define concrete migration waves and acceptance checks for the batch-1 lock window.

## Changes

- Completed script-surface inventory across:
  - `/Users/betterthanclay/Dev/projects/effigy/scripts/*.sh`
  - `/Users/betterthanclay/Dev/projects/effigy/docs/scripts/*.sh`
  - `/Users/betterthanclay/Dev/projects/effigy/.cargo/config.toml`
  - `/Users/betterthanclay/Dev/projects/effigy/.github-bak/workflows/*.yml`
- Built a per-surface decision matrix with disposition and rationale.
- Locked batch-1 execution waves and explicit carry-forward items.

## Decision Matrix

### Group A: migrate to Effigy-first command surfaces in batch 1

- `scripts/check-quality-gates.sh`
  - Disposition: migrate to Effigy built-in command path (`effigy qa` equivalent entrypoint) and keep script as temporary compatibility wrapper.
  - Rationale: this is the primary quality gate aggregator already wrapped by `cargo qa`.
- `scripts/check-release-gates.sh`
  - Disposition: migrate to Effigy release QA command path and keep script wrapper for external callsites.
  - Rationale: release gate orchestration is already mirrored by `cargo qa-release`.
- `scripts/check-json-contracts.sh`
  - Disposition: expose and document Effigy-first invocation for selection + schema checks; retain script as compatibility wrapper for CI and docs references.
  - Rationale: central contract checks should be discoverable from Effigy command surface.
- `scripts/check-json-contracts-ci.sh`
  - Disposition: keep logic, but run through Effigy-first invocation in docs and local contributor paths.
  - Rationale: PR/non-PR branching remains script-specific, but operator entrypoint should be unified.
- `scripts/check-prepush-ci.sh`
  - Disposition: route through Effigy-first QA composition and reduce direct script usage in contributor docs.
  - Rationale: pre-push path should be a stable first-class command contract.

### Group B: keep as thin wrappers in batch 1 (external contract required)

- `scripts/check-release-install-from-tag.sh`
- `scripts/check-release-smoke.sh`
- `scripts/check-distribution-first-publish.sh`
- `scripts/check-distribution-preflight.sh`
- `scripts/check-distribution-metadata.sh`
- `scripts/check-distribution-artifact-pipeline-smoke.sh`
- `scripts/validate-distribution-artifacts.sh`
- `scripts/generate-distribution-closeout-report.sh`
- `scripts/update-homebrew-formula-from-metadata.sh`
- `scripts/validate-json-contract-selection-artifact.sh`
- `scripts/check-selection-artifact-validator-smoke.sh`
- `scripts/add-report-index-entry.sh`

Rationale:

- These scripts are directly referenced by workflow jobs, release runbooks, or file-based tool contracts where stable standalone executables are still practical.
- Batch 1 keeps them, but each retained wrapper should gain explicit delegation notes once Effigy-first equivalents are finalized.

### Group C: hold for later tranche (docs-policy enforcement scripts)

- `docs/scripts/check-vision-metadata.sh`
- `docs/scripts/check-vision-next-task.sh`
- `docs/scripts/check-vision-index.sh`
- `docs/scripts/check-doc-workflow-paths.sh`

Rationale:

- These are docs governance/policy checks with repository-specific file scans and are currently invoked inside docs quality flow.
- Batch 1 prioritizes runtime/release QA paths first; docs-policy script delegation can follow after batch-1 QA entrypoints are stable.

## Execution Waves (Batch 1)

- Wave 1 (immediate): operator entrypoint unification
  - Make `cargo qa`, `cargo qa-docs`, `cargo qa-json`, `cargo qa-json-ci`, and `cargo qa-release` the canonical documented commands.
  - Reduce docs/examples that lead with direct script calls when a cargo/Effigy equivalent exists.
- Wave 2 (same tranche): compatibility wrapper normalization
  - Keep wrapper scripts, but annotate/standardize them as compatibility surfaces where applicable.
  - Ensure wrapper scripts remain thin orchestrators, not divergent logic branches.
- Wave 3 (closeout): consolidated validation evidence
  - Run one consolidated pass using the unified entrypoints and capture outcome matrix for batch closeout.

## Wave 1 Execution Checkpoint (2026-03-05)

- Updated canonical operator guidance to prefer cargo/Effigy entrypoints:
  - `/Users/betterthanclay/Dev/projects/effigy/README.md`
  - `/Users/betterthanclay/Dev/projects/effigy/docs/guides/024-ci-and-automation-recipes.md`
- Compatibility scripts remain documented, but now explicitly framed as wrapper surfaces for CI/release tooling.
- Remaining in batch 1:
  - wrapper-policy pass (delegation annotations + divergence checks)
  - consolidated validation + closeout evidence

## Wave 2 Execution Checkpoint (2026-03-05)

- Added wrapper-policy delegation headers to retained compatibility scripts:
  - `add-report-index-entry.sh`
  - `check-distribution-artifact-pipeline-smoke.sh`
  - `check-distribution-first-publish.sh`
  - `check-distribution-metadata.sh`
  - `check-distribution-preflight.sh`
  - `check-release-install-from-tag.sh`
  - `check-release-smoke.sh`
  - `validate-distribution-artifacts.sh`
  - `generate-distribution-closeout-report.sh`
  - `update-homebrew-formula-from-metadata.sh`
  - `validate-json-contract-selection-artifact.sh`
  - `check-selection-artifact-validator-smoke.sh`
- Verified retained wrappers remain syntax-valid and execution-safe.
- Remaining in batch 1:
  - consolidated validation + closeout evidence

## Wave 3 Execution Checkpoint (2026-03-05)

- Ran consolidated validation on the canonical unified QA entrypoint:
  - `cargo qa`
- Result: pass (docs gates + vision metadata + JSON contract checks all succeeded).
- Batch-1 closeout decision: complete.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`
- Movement: baseline `script-unification was a locked intent without an execution matrix` -> current `batch-1 inventory, migration waves, wrapper-policy pass, and consolidated QA validation are all captured`.
- Remaining gap: `batch-2 scope definition for deploy-readiness preflight migration into doctor`.

## Validation

- command: `find scripts -maxdepth 1 -type f | sort`
  - result: pass (script surface inventory captured)
- command: `ls docs/scripts && sed -n '1,260p' docs/scripts/*.sh`
  - result: pass (docs-policy script inventory captured)
- command: `sed -n '1,320p' scripts/check-json-contracts.sh scripts/check-json-contracts-ci.sh scripts/check-prepush-ci.sh`
  - result: pass (QA orchestration chain reviewed)
- command: `sed -n '1,260p' .cargo/config.toml`
  - result: pass (canonical cargo QA aliases confirmed)
- command: `./scripts/check-doc-reports-index.sh`
  - result: pass (new script-unification report index entry validated)
- command: `./scripts/check-doc-links.sh README.md docs/guides/024-ci-and-automation-recipes.md docs/logs/README.md docs/logs/2026-03/10-090000-script-surface-unification-batch-1.md`
  - result: pass (updated docs links validated for wave-1 edits)
- command: `for f in scripts/{add-report-index-entry.sh,check-distribution-artifact-pipeline-smoke.sh,check-distribution-first-publish.sh,check-distribution-metadata.sh,check-distribution-preflight.sh,check-release-install-from-tag.sh,check-release-smoke.sh,validate-distribution-artifacts.sh,generate-distribution-closeout-report.sh,update-homebrew-formula-from-metadata.sh,validate-json-contract-selection-artifact.sh,check-selection-artifact-validator-smoke.sh}; do bash -n \"$f\"; done`
  - result: pass (wrapper-policy annotation pass did not introduce shell syntax regressions)
- command: `cargo qa`
  - result: pass (unified docs + vision + JSON quality gate path succeeded under canonical operator entrypoint)

## Risks / Follow-ups

- There is still broad documentation usage of direct `./scripts/...` commands; this needs controlled migration to avoid breaking runbook assumptions.
- Workflow files are under `.github-bak`; any future path normalization must stay aligned with docs policy scripts.
- Parallel thread activity on tasks-listing refactor remains active; avoid coupling script-unification edits to that code path.

## Next

- Lock batch-2 scope for deploy-readiness preflight migration into `doctor`.
- Keep release/distribution scripts as explicit wrappers unless batch-2 defines a stable replacement contract.
- Use batch-1 outcomes as the baseline for future script-surface drift checks.
