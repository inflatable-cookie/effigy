# 011 - Watch Mode, Init, and Migrate (Phase 1)

Status: Complete
Owner: Platform
Created: 2026-02-28
Depends on: 001, 002, 003, 004, 008, 010

## 1) Problem

Effigy has strong task execution and diagnostics, but onboarding and iteration workflows still require manual setup:
- no first-class watch loop for file-triggered reruns with ownership safeguards,
- no `init` command to scaffold a valid baseline `effigy.toml`,
- no `migrate` helper to safely import existing scripts into manifest tasks.

Without these, adoption cost stays high and teams duplicate migration/watch conventions inconsistently.

## 2) Goals

- [x] Implement watch mode phase-1 for non-watcher task reruns.
- [x] Enforce explicit watch-owner policy to avoid nested watcher conflicts.
- [x] Add `effigy init` to generate valid baseline `effigy.toml` scaffolding.
- [x] Add `effigy migrate` phase-1 flow for `package.json` script import.
- [x] Ensure migrate supports preview + confirm before writes.
- [x] Keep migration non-destructive (source scripts preserved).

## 3) Non-Goals

- [ ] No heuristic watcher-ownership inference in phase 1.
- [ ] No source modification/deletion of `package.json` scripts.
- [ ] No migration from non-`package.json` sources in phase 1.
- [ ] No remote/shared-state dependency for watch internals.
- [ ] No schema-breaking manifest changes beyond Batch A/B contracts.

## 4) Start Gates

Batch C phase-1 starts only when:

- [ ] Gate 1: manifest schema freeze for Milestone 1 is stable.
  - reason: `init` and `migrate` must generate currently valid schema.
- [x] Gate 2: lock contract freeze is stable.
  - reason: watch mode must interoperate with lock scopes safely.
- [x] Gate 3 (preferred): doctor/json baseline freeze.
  - reason: reduce cross-batch command-surface churn while onboarding commands are added.

## 5) Baseline Scope

### Watch Mode (Phase 1)

- [x] File-triggered rerun for non-watcher tasks.
- [x] Explicit watch-owner policy to prevent nested watcher conflicts.
- [x] Debounce support.
- [x] Include/exclude glob controls.
- [x] Actionable diagnostics for ownership conflicts and invalid watch config.

### Init Helper (Phase 1)

- [x] Add `effigy init`.
- [x] Generate minimal valid `effigy.toml`.
- [x] Include commented examples for:
  - DAG-style task wiring,
  - managed dev task (`mode = "tui"`).
- [x] Ensure generated scaffold validates immediately with existing parser/contracts.

### Migrate Helper (Phase 1)

- [x] Add `effigy migrate`.
- [x] Read from `package.json` scripts only.
- [x] Provide preview output before applying writes.
- [x] Require explicit confirmation/apply step.
- [x] Preserve existing source scripts unchanged (non-destructive).

## 6) Execution Plan

### Phase 11.1 - Contract and CLI Surface
- [x] Define command UX and flags for `watch`, `init`, and `migrate`.
- [x] Add help surfaces and examples for all three.
- [x] Add JSON support decisions/shape for migrate preview output.

### Phase 11.2 - Watch Runtime (Owner-Policy First)
- [x] Implement watch loop runner with debounce and glob filtering.
- [x] Enforce explicit owner-policy guardrails for nested watchers.
- [x] Emit deterministic conflict diagnostics with remediation guidance.
- [x] Enforce lock-scope interop (`task:watch:<target>`) for concurrent watch-owner contention.

### Phase 11.3 - Init Scaffold
- [x] Implement scaffold writer with safe file existence handling.
- [x] Add baseline template with commented examples.
- [x] Validate generated output against manifest parser and docs contract.

### Phase 11.4 - Migrate (Package Scripts)
- [x] Parse `package.json` scripts into candidate task mappings.
- [x] Render preview table/diff-like summary.
- [x] Add explicit apply path for writing `effigy.toml` updates.
- [x] Preserve source scripts and annotate migration decisions.

### Phase 11.5 - Tests and Docs
- [x] Add contract tests for watch owner-policy and rerun behavior.
- [x] Add init snapshot tests for generated scaffold.
- [x] Add migrate preview/apply tests with non-destructive guarantees.
- [x] Publish usage guides and migration playbook with before/after examples.

## 7) Acceptance Criteria

- [x] `watch` reruns targeted tasks on changes with configurable debounce/globs.
- [x] Watch owner-policy prevents nested watcher conflicts deterministically.
- [x] `effigy init` creates a valid baseline `effigy.toml` scaffold.
- [x] `effigy migrate` can preview and apply `package.json` script import safely.
- [x] Migration does not mutate or delete `package.json` scripts.
- [x] Tests/docs cover phase-1 behavior and operator remediation paths.

## 8) Risks and Mitigations

- [ ] Risk: watch mode fights framework-native watchers.
  - Mitigation: explicit owner policy and fail-fast conflict messaging.
- [ ] Risk: generated init template drifts from active schema.
  - Mitigation: validate scaffold via tests against current parser/contracts.
- [ ] Risk: migrate output is noisy or unsafe.
  - Mitigation: preview-first flow, explicit apply gate, no source mutation.
- [ ] Risk: Batch C overlaps unresolved Batch A lock/schema work.
  - Mitigation: enforce start gates and sequence watch/runtime work after lock freeze.

## 9) Deliverables

- [x] Phase-1 watch mode with owner-policy safeguards.
- [x] `effigy init` baseline scaffold command.
- [x] `effigy migrate` package-script preview/apply helper.
- [x] Tests and docs for onboarding/watch/migration flows.
