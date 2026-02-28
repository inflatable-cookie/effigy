# 011 - Watch Mode, Init, and Migrate (Phase 1)

Status: Not Started
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

- [ ] Implement watch mode phase-1 for non-watcher task reruns.
- [ ] Enforce explicit watch-owner policy to avoid nested watcher conflicts.
- [ ] Add `effigy init` to generate valid baseline `effigy.toml` scaffolding.
- [ ] Add `effigy migrate` phase-1 flow for `package.json` script import.
- [ ] Ensure migrate supports preview + confirm before writes.
- [ ] Keep migration non-destructive (source scripts preserved).

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
- [ ] Gate 2: lock contract freeze is stable.
  - reason: watch mode must interoperate with lock scopes safely.
- [x] Gate 3 (preferred): doctor/json baseline freeze.
  - reason: reduce cross-batch command-surface churn while onboarding commands are added.

## 5) Baseline Scope

### Watch Mode (Phase 1)

- [ ] File-triggered rerun for non-watcher tasks.
- [ ] Explicit watch-owner policy to prevent nested watcher conflicts.
- [ ] Debounce support.
- [ ] Include/exclude glob controls.
- [ ] Actionable diagnostics for ownership conflicts and invalid watch config.

### Init Helper (Phase 1)

- [ ] Add `effigy init`.
- [ ] Generate minimal valid `effigy.toml`.
- [ ] Include commented examples for:
  - DAG-style task wiring,
  - managed dev task (`mode = "tui"`).
- [ ] Ensure generated scaffold validates immediately with existing parser/contracts.

### Migrate Helper (Phase 1)

- [ ] Add `effigy migrate`.
- [ ] Read from `package.json` scripts only.
- [ ] Provide preview output before applying writes.
- [ ] Require explicit confirmation/apply step.
- [ ] Preserve existing source scripts unchanged (non-destructive).

## 6) Execution Plan

### Phase 11.1 - Contract and CLI Surface
- [ ] Define command UX and flags for `watch`, `init`, and `migrate`.
- [ ] Add help surfaces and examples for all three.
- [ ] Add JSON support decisions/shape for migrate preview output.

### Phase 11.2 - Watch Runtime (Owner-Policy First)
- [ ] Implement watch loop runner with debounce and glob filtering.
- [ ] Enforce explicit owner-policy guardrails for nested watchers.
- [ ] Emit deterministic conflict diagnostics with remediation guidance.

### Phase 11.3 - Init Scaffold
- [ ] Implement scaffold writer with safe file existence handling.
- [ ] Add baseline template with commented examples.
- [ ] Validate generated output against manifest parser and docs contract.

### Phase 11.4 - Migrate (Package Scripts)
- [ ] Parse `package.json` scripts into candidate task mappings.
- [ ] Render preview table/diff-like summary.
- [ ] Add explicit apply path for writing `effigy.toml` updates.
- [ ] Preserve source scripts and annotate migration decisions.

### Phase 11.5 - Tests and Docs
- [ ] Add contract tests for watch owner-policy and rerun behavior.
- [ ] Add init snapshot tests for generated scaffold.
- [ ] Add migrate preview/apply tests with non-destructive guarantees.
- [ ] Publish usage guides and migration playbook with before/after examples.

## 7) Acceptance Criteria

- [ ] `watch` reruns targeted tasks on changes with configurable debounce/globs.
- [ ] Watch owner-policy prevents nested watcher conflicts deterministically.
- [ ] `effigy init` creates a valid baseline `effigy.toml` scaffold.
- [ ] `effigy migrate` can preview and apply `package.json` script import safely.
- [ ] Migration does not mutate or delete `package.json` scripts.
- [ ] Tests/docs cover phase-1 behavior and operator remediation paths.

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

- [ ] Phase-1 watch mode with owner-policy safeguards.
- [ ] `effigy init` baseline scaffold command.
- [ ] `effigy migrate` package-script preview/apply helper.
- [ ] Tests and docs for onboarding/watch/migration flows.
