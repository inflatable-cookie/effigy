# Script Surface Built-ins Migration Plan

Status: complete
Created: 2026-03-11
Roadmap: post-release hardening / script-surface reduction
Batch: script-surface-builtins-migration-plan

## Summary

- Reviewed the active `scripts/` surface after the release/changelog work was
  fully integrated into Effigy.
- Defined the architecture boundary for future migration:
  - generic logic -> Effigy built-ins
  - repo policy -> `effigy.toml` / contract files / docs config
  - external compatibility entrypoints -> thin wrappers only
- Classified every current `scripts/*` entrypoint by disposition so follow-on
  batches can migrate deliberately instead of replacing shell with another
  ad-hoc scripting layer.

## Changes

- Established the decision rule for new work:
  - do not add new nontrivial repo logic in shell when the logic can live in a
    reusable built-in plus repo config
- Grouped current script surfaces into four categories:
  - built-in candidates
  - built-in candidates with repo-specific config inputs
  - thin wrappers to retain
  - local-machine helpers to retain

## Decision Framework

Use these tests before moving a script into Effigy:

1. Can another repo reuse the capability by changing config or input files only?
   - If yes, it is a built-in candidate.
2. Does the script mainly sequence checks that already have stable commands?
   - If yes, it should become an `effigy.toml` task chain, not a new built-in.
3. Does the script exist mainly for CI/workflow/path/bootstrap compatibility?
   - If yes, keep it as a thin wrapper and remove business logic from it.
4. Does the script encode Effigy-repo policy that would be meaningless in
   another repo without code changes?
   - If yes, keep the policy declarative and build only the generic engine into
     Effigy.

## Current Classification

### Group A - Highest-value generic built-in candidates

- `scripts/check-doc-links.sh`
  - Target: generic docs built-in
  - Why: markdown link validation is reusable across repos
  - Config boundary: docs roots / ignore patterns
- `scripts/check-doc-json-examples.sh`
  - Target: generic docs built-in
  - Why: “extract markdown code blocks and validate JSON snippets” is reusable
  - Config boundary: file path, section selector, expected keys/rules
- `scripts/check-doc-logs-index.sh`
  - Target: generic docs/index built-in
  - Why: “index matches discovered files” is a reusable file-index pattern
  - Config boundary: index path, glob roots, link prefix format
- `scripts/validate-json-contract-selection-artifact.sh`
  - Target: generic contracts built-in
  - Why: file-shaped JSON artifact validation is reusable
  - Config boundary: contract file path, artifact path
- `scripts/check-selection-artifact-validator-smoke.sh`
  - Target: generic contracts built-in or test fixture
  - Why: this is really a validator fixture harness
  - Config boundary: fixture files should move into tests
- `scripts/check-distribution-metadata.sh`
  - Target: generic release/distribution built-in
  - Why: checks package metadata, required docs, and workflow wiring
  - Config boundary: required files, workflow path, expected workflow markers
- `scripts/validate-distribution-artifacts.sh`
  - Target: generic release/distribution built-in
  - Why: validates artifact bundle shape from log files
  - Config boundary: required log patterns, optional channels
- `scripts/generate-distribution-closeout-log.sh`
  - Target: generic release/distribution built-in
  - Why: deterministic report generation from validated artifact inputs
  - Config boundary: template fields, owner/tag/artifact-dir inputs

### Group B - Likely built-ins plus task composition

- `scripts/check-quality-gates.sh`
  - Target: split into built-ins plus `effigy.toml` task composition
  - Why: it is an aggregator, not one coherent primitive
  - Recommended replacement:
    - built-in docs check command(s)
    - built-in contracts check command(s)
    - task aliases such as `qa:docs`, `qa:json`, `qa`
- `scripts/check-json-contracts.sh`
  - Target: generic contracts built-in
  - Why: selection + execution logic already behaves like a first-class command
  - Config boundary: schema index path and selection rules
- `scripts/check-json-contracts-ci.sh`
  - Target: very thin wrapper or CI-mode flag on the built-in
  - Why: almost all value is argument selection around PR vs non-PR mode
- `scripts/check-distribution-preflight.sh`
  - Target: task chain over built-ins
  - Why: orchestrates docs, metadata, and smoke checks
  - Config boundary: which checks run and summary output path
- `scripts/check-distribution-first-publish.sh`
  - Target: task chain or purpose-built built-in only if we want first-class
    artifact-capture/reporting
  - Why: mostly orchestration of install validations and artifact logs
- `scripts/check-distribution-artifact-pipeline-smoke.sh`
  - Target: tests/fixtures, not a long-term operator script
  - Why: this is smoke coverage for the artifact-validator/report-generator path
- `scripts/add-log-index-entry.sh`
  - Target: generic docs/index built-in or docs-focused helper task
  - Why: deterministic index insertion is reusable, but more of a helper than a
    core command

### Group C - Keep as thin wrappers for now

- `scripts/check-release-gates.sh`
  - Already a wrapper over built-in release commands
- `scripts/check-release-install-from-tag.sh`
  - Already a wrapper over built-in release commands
- `scripts/prepare-release.sh`
  - Legacy compatibility wrapper; no new logic should go here
- `scripts/check-release-smoke.sh`
  - Keep short-term as a wrapper/helper unless we decide smoke verification
    should become part of a generalized built-in binary-check surface
- `scripts/check-prepush-ci.sh`
  - Keep temporarily as a convenience wrapper while canonical task aliases are
    strengthened

### Group D - Keep as local-machine helpers

- `scripts/install-local-bin-links.sh`
  - This is machine bootstrap glue, not product logic
- `scripts/effigy-dev`
  - This is a local convenience launcher, not a candidate built-in

## Lock-in Boundary

The safe built-in boundary is:

- built-in implements a generic engine
- repo-specific policy lives in config or data files
- wrapper exists only when external entrypoints still matter

Examples:

- docs link checking is generic
- “Effigy vision metadata” is repo-specific policy
- JSON artifact validation is generic
- Effigy’s exact schema inventory is repo-specific data
- distribution artifact bundle validation is generic
- Effigy’s exact required workflow/doc files are repo-specific config

So the right built-ins are things like:

- `effigy docs check-links`
- `effigy docs check-index`
- `effigy contracts check-json`
- `effigy contracts validate-selection`
- `effigy distribution validate-metadata`
- `effigy distribution validate-artifacts`
- `effigy distribution generate-closeout`

Not things like:

- `effigy check-effigy-docs`
- `effigy validate-effigy-json-contracts`
- hardcoded `docs/logs/README.md` or `docs/contracts/json-schema-index.json`

## Recommended Migration Waves

### Wave 1 - Docs and contracts

- migrate:
  - `check-doc-links.sh`
  - `check-doc-json-examples.sh`
  - `check-doc-logs-index.sh`
  - `check-json-contracts.sh`
  - `check-json-contracts-ci.sh`
  - `validate-json-contract-selection-artifact.sh`
  - `check-selection-artifact-validator-smoke.sh`
- reason:
  - high reuse potential
  - relatively low release risk
  - immediate reduction in shell-based QA logic

### Wave 2 - Distribution validation/reporting

- migrate:
  - `check-distribution-metadata.sh`
  - `validate-distribution-artifacts.sh`
  - `generate-distribution-closeout-log.sh`
  - `check-distribution-artifact-pipeline-smoke.sh`
- reason:
  - these are policy-heavy and easier to reason about in typed Rust than shell

### Wave 3 - Orchestration cleanup

- reduce shell aggregators to task chains or tiny wrappers:
  - `check-quality-gates.sh`
  - `check-distribution-preflight.sh`
  - `check-distribution-first-publish.sh`
  - `check-prepush-ci.sh`
- reason:
  - after Waves 1 and 2, most real logic should already be owned elsewhere

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- Movement: baseline `script cleanup was a broad preference` -> current
  `script surfaces are classified by reusable built-in candidate vs repo policy
  vs wrapper/helper with an explicit migration order`
- Remaining gap: `implement Wave 1 built-ins and convert the first
  script-backed QA paths to Effigy-native command surfaces`

## Validation Performed

- command: `find scripts -maxdepth 1 -type f | sort`
  - result: pass
- command: `for f in scripts/*.sh scripts/effigy-dev; do sed -n '1,220p' "$f"; done`
  - result: pass
- command: `sed -n '1,260p' docs/logs/archive/2026-03/10-090000-script-surface-unification-batch-1.md`
  - result: pass
- command: `rg -n "scripts/[^\\s'\\\")]+" README.md docs .github effigy.toml src tests Cargo.toml`
  - result: pass

## Risks

- docs-policy scripts under `docs/scripts/` are a separate surface and should
  not be silently folded into this migration without deciding whether they are
  generic enough for built-ins
- workflow and runbook references to `./scripts/...` remain widespread, so
  migration has to preserve compatibility while operator-facing guidance shifts
- some distribution scripts may look generic but still rely on Effigy-specific
  file layouts; that needs a config-first design rather than hardcoded paths

## Next Task

- Implement Wave 1 as a single batch:
  - define the generic command/config boundary for docs and contracts checks
  - build the first Effigy-native docs/contracts command surface
  - turn the corresponding `scripts/*.sh` entrypoints into thin compatibility
    wrappers over those built-ins
