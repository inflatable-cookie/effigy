# 028 - Script Surface Reduction and Built-ins

Generation: `g01`

Status: Complete
Owner: Platform
Created: 2026-03-11
Depends on: 008, 026, 027

## Vision Alignment

This roadmap reduces Effigy's reliance on repository shell scripts by moving
durable validation and operational logic into first-class Effigy command
surfaces, while keeping repository-specific policy declarative and preserving
thin compatibility wrappers where external entrypoints still matter.

The goal is not "rewrite bash in Rust." The goal is:

- generic engines live in Effigy
- repo policy lives in config and data
- shell remains only as a compatibility boundary or local-machine glue

## Primary Tags

- `ROUTE`
- `CONTRACT`
- `OPERATE`
- `MAINT`

## Target Envelope

- Effigy owns the reusable logic currently trapped in `scripts/` for docs QA,
  JSON contract validation, and distribution validation/report generation.
- `effigy.toml` task composition replaces shell aggregators wherever the script
  is only sequencing existing checks.
- Repo-specific policy does not get hardcoded into built-ins; it remains in
  config files, contract files, or task definitions.
- Legacy script paths remain available as thin wrappers until migration is
  complete and external callers are updated.
- New nontrivial repo automation logic should not be added to `scripts/`
  unless there is a strong external-boundary reason.

## Vision Target Delta

- Moved from `repo-critical QA and distribution logic spread across many shell
  entrypoints` toward `Effigy-native command surfaces plus declarative task
  composition, with shell reduced to wrappers and bootstrap helpers`.

## Source of Truth

This roadmap is based on the classification and migration analysis in:

- [`../../logs/2026-03/11-202500-script-surface-builtins-migration-plan.md`](../../logs/2026-03/11-202500-script-surface-builtins-migration-plan.md)

That log captures the per-script classification. This roadmap turns that
analysis into executable milestone waves.

## Design Rules

### 1. Build generic engines, not Effigy-repo one-offs

A capability is a built-in candidate only if another repo could reuse it by
changing config or inputs rather than patching Effigy itself.

Good built-in examples:

- docs link checking
- markdown JSON example validation
- file-index consistency checking
- JSON selection artifact validation
- distribution artifact validation
- closeout report generation from declared inputs

Bad built-in examples:

- hardcoded `docs/logs/README.md` assumptions
- hardcoded "vision metadata" semantics
- hardcoded Effigy schema inventory rules
- hardcoded workflow file names where a config surface should exist

### 2. Prefer task composition over built-in orchestration blobs

If a shell script mainly sequences existing commands, it should usually become:

- a set of smaller built-ins, plus
- an `effigy.toml` task chain

This keeps orchestration visible and configurable instead of burying it in a
single opaque command.

### 3. Keep wrappers only where they provide external stability

Scripts are still acceptable when they are:

- CI compatibility entrypoints
- bootstrap helpers
- local PATH/dev convenience wrappers
- legacy migration surfaces with documented retirement conditions

But those wrappers should contain minimal logic and delegate immediately.

## Script Groups

### Group A - Generic built-in candidates

- `scripts/check-doc-links.sh`
- `scripts/check-doc-json-examples.sh`
- `scripts/check-doc-logs-index.sh`
- `scripts/validate-json-contract-selection-artifact.sh`
- `scripts/check-selection-artifact-validator-smoke.sh`
- `scripts/check-distribution-metadata.sh`
- `scripts/validate-distribution-artifacts.sh`
- `scripts/generate-distribution-closeout-log.sh`

### Group B - Built-ins plus task composition

- `scripts/check-quality-gates.sh`
- `scripts/check-json-contracts.sh`
- `scripts/check-json-contracts-ci.sh`
- `scripts/check-distribution-preflight.sh`
- `scripts/check-distribution-first-publish.sh`
- `scripts/check-distribution-artifact-pipeline-smoke.sh`
- `scripts/add-log-index-entry.sh`
- `scripts/check-prepush-ci.sh`

### Group C - Thin compatibility wrappers

Resolved:
- the release wrapper cluster has been retired from the live repo
- the native replacement surfaces are:
  - `effigy release gates`
  - `effigy release verify-install`
  - `effigy release prepare`
  - `effigy smoke:release`
  - `effigy bootstrap:local`

### Group D - Local-machine helpers

- `scripts/effigy-dev`

## Wave 1 - Docs and Contracts Built-ins

Deliver the first generic built-in batch by migrating docs and JSON-contract
logic out of shell.

Target command surface shape:

```text
effigy docs check-links
effigy docs check-json-examples
effigy docs check-index
effigy contracts check-json
effigy contracts validate-selection
```

Config direction:

- docs inputs come from explicit paths/globs and optional config sections
- contract inputs come from schema index paths and contract files
- CI mode is a flag or mode on the built-in, not a separate logic copy

Tasks:

- [x] Design docs built-in command family and config boundary
- [x] Design contracts built-in command family and config boundary
- [x] Implement link checking as a built-in
- [x] Implement markdown JSON example validation as a built-in
- [x] Implement logs/index consistency checking as a built-in
- [x] Implement JSON contract selection/check execution as a built-in
- [x] Implement selection-artifact validation as a built-in
- [x] Move smoke-style validator fixtures into Rust tests where appropriate
- [x] Convert `scripts/check-doc-links.sh` into a thin wrapper
- [x] Convert `scripts/check-doc-json-examples.sh` into a thin wrapper
- [x] Convert `scripts/check-doc-logs-index.sh` into a thin wrapper
- [x] Convert `scripts/check-json-contracts.sh` into a thin wrapper
- [x] Convert `scripts/check-json-contracts-ci.sh` into a thin wrapper
- [x] Convert `scripts/validate-json-contract-selection-artifact.sh` into a thin wrapper
- [x] Reduce `scripts/check-selection-artifact-validator-smoke.sh` to either a
  test helper or wrapper-only surface
- [x] Update `effigy.toml` tasks and docs to lead with Effigy-native commands

Acceptance:

- docs QA and JSON-contract QA can run through Effigy-native command surfaces
- corresponding shell scripts delegate rather than own logic
- no hardcoded Effigy-repo policy is baked into the new built-ins without a
  config boundary

## Wave 2 - Distribution Validation and Reporting

Move distribution validation/report generation into typed built-ins.

Target command surface shape:

```text
effigy distribution validate-metadata
effigy distribution validate-artifacts
effigy distribution generate-closeout
```

Config direction:

- required workflow/doc/package markers are declarative
- required artifact log patterns are declarative
- closeout report fields are derived from validated inputs, not handwritten
  shell interpolation

Tasks:

- [x] Design distribution built-in command family and config boundary
- [x] Implement distribution metadata validation as a built-in
- [x] Implement artifact bundle validation as a built-in
- [x] Implement closeout log generation as a built-in
- [x] Re-home artifact-pipeline smoke coverage into tests and/or built-in QA
- [x] Convert `scripts/check-distribution-metadata.sh` into a thin wrapper
- [x] Convert `scripts/validate-distribution-artifacts.sh` into a thin wrapper
- [x] Convert `scripts/generate-distribution-closeout-log.sh` into a thin wrapper
- [x] Reduce `scripts/check-distribution-artifact-pipeline-smoke.sh` to a test
  fixture harness or wrapper-only surface
- [x] Update distribution runbooks and CI recipes to lead with Effigy-native
  commands where appropriate

Acceptance:

- distribution validation/report generation no longer depends on shell as the
  primary implementation language
- wrappers remain only for compatibility
- runbook behavior remains unchanged from an operator perspective

## Wave 3 - Orchestration Cleanup

Once Waves 1 and 2 exist, shrink the remaining shell aggregators into task
composition and tiny wrappers.

Tasks:

- [x] Replace `scripts/check-quality-gates.sh` with native task composition
- [x] Replace `scripts/check-distribution-preflight.sh` with native task
  composition plus summary output handling
- [x] Replace `scripts/check-distribution-first-publish.sh` with task
  composition and only the minimum artifact-capture wrapper surface if needed
  - Outcome: wrapper now delegates tag verification, artifact summary, and
    artifact validation to native Effigy commands and retains only real
    publish/install/Homebrew side effects plus per-step log capture
- [x] Replace `scripts/check-prepush-ci.sh` with canonical task aliases and a
  very small wrapper, or remove it if redundant
- [x] Audit workflow/docs references to make sure operator guidance leads with
  Effigy-native commands rather than `./scripts/...`

Acceptance:

- shell aggregators no longer own meaningful repo logic
- orchestration is visible in `effigy.toml` and built-ins rather than bash
- the remaining `scripts/` directory is mostly wrappers and local-machine glue

Completion note:

- Wave 3 is complete. The only intentionally nontrivial remaining wrapper is
  `scripts/check-distribution-first-publish.sh`, which is retained as the
  external boundary for real publish/install side effects rather than as a home
  for reusable validation logic.
- The remaining `docs/scripts/check-vision-*.sh` surface is intentionally not
  part of this roadmap closeout. Those checks are mostly Effigy-specific
  docs-policy enforcement and should only migrate further behind a minimal
  config boundary instead of being hardcoded into generic built-ins.
- Follow-on design note:
  [`../../logs/2026-03/12-093000-docs-policy-config-boundary.md`](../../logs/2026-03/12-093000-docs-policy-config-boundary.md)

## Non-Goals

- Do not migrate local bootstrap helpers just for ideological purity
- Do not introduce Python/Node as a default replacement for shell
- Do not hardcode Effigy-repo governance rules into built-ins without a config
  layer
- Do not break workflow/runbook compatibility while migration is in flight

## Validation Strategy

Each wave should complete with:

- targeted Rust tests for new built-ins
- compatibility-wrapper parity tests where wrappers remain
- docs QA updates and proof that the command surface is discoverable
- `git diff --check`

For Wave 1 specifically:

- `qa:docs`
- JSON-contract validation coverage
- wrapper parity for docs/contracts scripts

For Wave 2 specifically:

- metadata/artifact validation coverage
- generated closeout report fixture tests
- runbook parity for distribution flows

## Next Batch Recommendation

Start with Wave 1 as one meaningful batch:

1. define the docs/contracts built-in command surface
2. implement the reusable core checks
3. convert the corresponding `scripts/*.sh` files into thin wrappers
4. update docs and task composition to make the new Effigy-native paths canonical
