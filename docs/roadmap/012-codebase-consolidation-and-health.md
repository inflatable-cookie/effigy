# 012 - Codebase Consolidation and Health

Status: In Progress
Owner: Platform
Created: 2026-03-01
Depends on: 001-011

## 1) Problem

Effigy shipped major feature breadth quickly across runner, doctor, managed runtime, and JSON contracts. The codebase now has several concentrated complexity hotspots that increase maintenance cost and bug risk:
- oversized multi-responsibility files,
- repeated control-flow and rendering branches,
- parsing/validation logic mixed with command orchestration,
- duplicated command/result shaping across text and JSON paths.

Without a consolidation pass, new features will continue to compound complexity and slow iteration.

## 2) Goals

- [ ] Reduce highest-risk god files by extracting cohesive helpers/modules.
- [ ] Remove duplicated branch logic in builtin command flows.
- [ ] Separate parsing/validation from rendering and execution concerns.
- [ ] Preserve existing CLI/JSON contracts while refactoring internals.
- [ ] Maintain or improve targeted test coverage for changed paths.

## 3) Non-Goals

- [ ] No user-visible command-surface redesign in this roadmap.
- [ ] No schema-breaking manifest changes.
- [ ] No broad rewrite of the runner architecture in a single batch.

## 4) Deep Analysis Findings (2026-03-01)

Code volume snapshot:
- Rust source total: 22,390 lines.
- Largest production hotspots:
  - `src/runner/doctor.rs` (2,006)
  - `src/runner/builtin/test.rs` (1,181)
  - `src/runner/managed.rs` (1,128)
  - `src/runner/mod.rs` (1,082)

Structural issues identified:
- `src/runner/builtin/test.rs` contains long branching paths for suite selection, recovery messaging, and plan rendering; repeated fallback rendering logic existed in multiple branches.
- `src/runner/doctor.rs` combines manifest scanning, schema validation, fixer execution, task execution, and text/json rendering in one file.
- `src/runner/managed.rs` blends plan resolution, DAG scheduling, invocation rendering, and runtime execution in one module.
- `src/runner/mod.rs` centralizes a large error enum and broad orchestration logic, creating high coupling between submodules.

## 5) Execution Plan

### Batch 12.1 - Builtin Test Flow Consolidation
- [x] Extract suite-selection logic into dedicated helper flow.
- [x] Consolidate plan-recovery/error branching into one helper.
- [x] Extract plan rendering into a dedicated function.
- [x] Keep output text and JSON contract behavior unchanged.
- [x] Validate with targeted runner tests for plan/recovery/filter paths.

### Batch 12.2 - Doctor Module Decomposition
- [ ] Split doctor checks into focused modules (`manifest`, `environment`, `references`, `health`).
- [ ] Keep a thin orchestration layer in `doctor.rs`.
- [ ] Reuse shared finding/status aggregation helpers.
- [ ] Preserve explain/fix/json behavior contracts.

### Batch 12.3 - Managed Runtime Separation
- [ ] Separate DAG scheduling/policy rendering from process launch execution.
- [ ] Extract task-reference resolution and invocation rendering into utilities.
- [ ] Add tests around dependency-cycle diagnostics and policy wrapping parity.

### Batch 12.4 - Runner Error/Rendering Cleanup
- [ ] Break `RunnerError` formatting and command rendering helpers into smaller components.
- [ ] Reduce repeated JSON encode failure handling with small utilities.
- [ ] Keep all public command outputs stable.

## 6) Acceptance Criteria

- [ ] At least three hotspot modules reduced in branch complexity and responsibility width.
- [ ] Existing contract tests for builtin test and doctor JSON outputs remain green.
- [ ] No regressions in targeted behavioral tests for suite selection, ambiguity recovery, and filter hints.
- [ ] Roadmap batches produce smaller, reviewable diffs with clear ownership boundaries.

## 7) Risks and Mitigations

- [ ] Risk: refactor changes user-visible messaging.
  - Mitigation: keep literal output strings stable and assert via tests.
- [ ] Risk: modularization introduces cross-file churn and merge friction.
  - Mitigation: ship in narrow, sequential batches with focused tests.
- [ ] Risk: hidden dependencies between runner modules.
  - Mitigation: introduce small helper interfaces before moving behavior.

## 8) Deliverables

- [x] Consolidated builtin test flow internals (batch 12.1).
- [ ] Doctor decomposition with preserved command contracts.
- [ ] Managed runtime decomposition with stable behavior.
- [ ] Runner core cleanup with reduced repetition and clearer ownership.
