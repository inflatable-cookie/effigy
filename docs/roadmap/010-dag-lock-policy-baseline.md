# 010 - DAG Lock and Policy Baseline

Status: In Progress
Owner: Platform
Created: 2026-02-28
Depends on: 001, 004, 005, 008, 009

## 1) Problem

Effigy supports simple linear task sequencing and managed dev profiles, but lacks a unified baseline for:
- dependency-graph execution (DAG),
- lock-based collision prevention,
- explicit execution policy controls (timeouts/retries/fail-fast).

Without this baseline, orchestration behavior becomes ambiguous in complex workspaces, and conflicting runs can produce unreliable outcomes.

## 2) Goals

- [ ] Add DAG-capable task execution while keeping current linear task chains valid.
- [ ] Enforce deterministic graph validation (cycle detection, missing node references).
- [ ] Support inferred parallel execution with bounded concurrency.
- [ ] Introduce explicit node-level execution policy (timeout, retry, fail-fast behavior).
- [ ] Introduce lock scopes and stale-lock recovery to prevent conflicting runs.
- [ ] Provide actionable error output for graph, policy, and lock failures.

## 3) Non-Goals

- [ ] No watch-mode implementation in phase 010.
- [ ] No remote/distributed lock backend in phase 010.
- [ ] No caching/up-to-date check implementation in phase 010.
- [ ] No breaking rewrite of existing linear `run = [{ task = "..." }, ...]` semantics.

## 4) Baseline Decisions

These are locked as phase defaults unless explicitly superseded:

- DAG syntax extends existing task syntax; current linear chains remain valid.
- Parallelism is inferred from dependency edges, with bounded max concurrency.
- Default failure mode is `fail_fast = true`.
- Retry/timeout policy is node-level first.
- Lock storage is filesystem-based under `.effigy/locks`.
- Lock scopes: `workspace`, `task:<name>`, `profile:<task>/<profile>`.
- Stale lock handling: auto-recover dead PID locks plus explicit manual unlock path.

### Compatibility with Managed `concurrent` Schema

- Existing managed task schema remains canonical for TUI fanout:
  - `mode = "tui"`
  - `concurrent = [...]`
  - `[tasks.<name>.profiles.<profile>]` profile overrides
- Phase 010 does not deprecate or replace the managed `concurrent` model.
- DAG/policy work applies to orchestration semantics and validation/runtime safety; managed TUI plan shape is preserved.
- Locking introduces intentional run-collision blocking across configured scopes; this may prevent conflicting simultaneous managed runs, but is not a schema break.
- Any future change to managed task schema must be handled in a separate roadmap item with explicit migration notes.

## 5) Execution Plan

### Phase 10.1 - Graph Model and Parser
- [ ] Define DAG-capable task shape in manifest model (backward-compatible with current linear chains).
- [ ] Parse dependency metadata into internal graph structure.
- [ ] Emit actionable parse errors for malformed DAG declarations.

### Phase 10.2 - Graph Validation
- [ ] Validate missing dependency node references before execution.
- [ ] Add deterministic cycle detection (including self-cycle and indirect cycle).
- [ ] Surface concise cycle/missing-node evidence in error output.

### Phase 10.3 - Scheduler and Parallel Execution
- [ ] Implement dependency-safe scheduler using inferred ready sets.
- [ ] Add bounded concurrency control.
- [ ] Keep deterministic ordering for nodes that become ready simultaneously.

### Phase 10.4 - Execution Policy Layer
- [ ] Add node-level timeout policy.
- [ ] Add node-level retry policy.
- [ ] Enforce default fail-fast behavior with explicit override path.
- [ ] Ensure policy failures surface stable diagnostics.

### Phase 10.5 - Locking Baseline
- [ ] Implement lock acquisition/release for workspace/task/profile scopes.
- [ ] Add lock conflict diagnostics (holder PID, scope, start time, remediation guidance).
- [ ] Add stale-lock recovery for dead PID ownership.
- [ ] Add explicit manual unlock command/path for operator override.

### Phase 10.6 - Contracts, Tests, and Docs
- [ ] Add contract tests for DAG validation, scheduling order, policy behavior, and lock collisions.
- [ ] Add stale-lock recovery tests (dead PID reclaimed, live PID preserved).
- [ ] Update docs with compact DAG examples and lock/policy behavior guidance.

## 6) Acceptance Criteria

- [ ] Existing linear sequence tasks execute unchanged.
- [ ] DAG cycles and missing dependencies fail fast with actionable evidence.
- [ ] Parallel execution respects dependencies and concurrency bounds.
- [ ] Policy controls (timeout/retry/fail-fast) are test-covered and deterministic.
- [ ] Conflicting runs are blocked by lock policy with clear unlock guidance.
- [ ] Stale locks are safely recovered without manual intervention when owner PID is dead.

## 7) Risks and Mitigations

- [ ] Risk: DAG schema introduces migration confusion.
  - Mitigation: preserve current chain syntax and document DAG extensions as additive.
- [ ] Risk: lock collisions produce false positives.
  - Mitigation: include PID liveness checks and scope-specific lock files.
- [ ] Risk: retry/timeout policy causes hidden behavior.
  - Mitigation: keep defaults explicit, emit policy evidence in failure output, and avoid implicit heuristics.

## 8) Deliverables

- [ ] DAG-capable parser + validator.
- [ ] Dependency-aware scheduler with inferred parallelism and bounded concurrency.
- [ ] Node-level timeout/retry/fail-fast execution policy layer.
- [ ] Filesystem lock model with scope isolation and stale-lock recovery.
- [ ] Test suite and docs updates for DAG/lock/policy baseline.
