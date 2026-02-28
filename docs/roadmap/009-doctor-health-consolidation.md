# 009 - Doctor Health Consolidation

Status: Complete
Owner: Platform
Created: 2026-02-28
Depends on: 001, 002, 003, 005, 008

## 1) Problem

Effigy currently splits health behavior across `repo-pulse`, a built-in `health` alias, and ad-hoc project tasks. The result is noisy output, duplicated command surface, and unclear remediation flow. We need one canonical `doctor` command that performs actionable checks, can apply safe fixes, and can invoke project-owned health checks when present.

## 2) Goals

- [x] Make `effigy doctor` the only built-in health/remediation command surface.
- [x] Keep output remediation-first: each failure includes concrete next actions.
- [x] Consolidate high-signal checks from `repo-pulse`; remove low-signal noise.
- [x] Add safe `--fix` behavior for unambiguous, low-risk remediations.
- [x] Include machine-readable output for `doctor` aligned with roadmap 008 JSON contracts.
- [x] Add implicit catalog/root `health` task detection and execution during doctor sweep.
- [x] Extend manifest diagnostics to report unsupported/unknown TOML settings and invalid values, not only parse failures.

## 3) Non-Goals

- [x] No compatibility alias retention for `health` or `repo-pulse` after migration.
- [x] No automatic edits for ambiguous or potentially destructive config changes.
- [x] No plugin system for arbitrary third-party doctor checks in phase 009.
- [x] No replacement of project-specific `health` task logic with built-in heuristics.

## 4) UX Contract

Primary command:
- `effigy doctor`

Options:
- `effigy doctor --json`
- `effigy doctor --fix`
- `effigy doctor --repo <PATH>`
- `effigy doctor <task> <args>` (explain mode)

Behavior:
- `doctor` runs grouped checks in deterministic order and prints a summary with pass/warn/error counts.
- `doctor` emits per-finding: stable check id, severity, evidence, remediation, and fix availability.
- `doctor --fix` applies only checks explicitly marked safe and reports each attempted fix.
- `doctor` implicitly checks whether a catalog/root `health` task exists and runs it if present.
- `doctor <task> <args>` reports task-resolution candidates, selection evidence/mode, ambiguity conditions, and deferral reasoning.
- `doctor <task> <args> --json` emits `effigy.doctor.explain.v1` for machine-readable explainability.
- `doctor` exits non-zero when at least one `error` finding exists.
- `doctor` is the only health-style built-in command; `health` and `repo-pulse` are removed.

## 5) Check Groups and Actions

Group: Environment
- `environment.tools.required`: validate required executables from detected task graph (`cargo`, `rustc`, `bun`/`pnpm`/`npm`/`node`).
- `environment.tools.optional`: suggest optional tools for improved flows (for example `cargo-nextest`) without hard-failing.

Group: Workspace and Manifest Integrity
- `workspace.root-resolution`: verify resolved root and resolution evidence.
- `manifest.parse`: validate TOML parse across discovered `effigy.toml` files.
- `manifest.schema.unsupported_key`: flag unknown/unsupported keys with exact path (`tasks.foo.bar`).
- `manifest.schema.unsupported_value`: flag invalid enum/value forms with accepted alternatives.
- `manifest.conflicts`: detect duplicate aliases/task-key collisions/reserved-name conflicts.

Group: Task and Orchestration Validity
- `tasks.references.resolve`: validate all `task:` edges resolve.
- `tasks.dag.cycle`: detect cyclic task dependencies with minimal cycle evidence.
- `tasks.dag.invalid_parallel`: catch illegal parallel declarations/missing nodes.
- `managed.profile.validity`: validate `mode=tui` profile/process/tab config integrity.

Group: Execution Readiness
- `locks.state`: detect active lock conflicts and stale locks.
- `deferral.contract`: validate fallback command definitions.
- `watch.ownership`: detect nested watcher ownership conflicts.

Group: Project Health Delegation
- `health.task.discovery`: discover root or catalog `health` task target.
- `health.task.execute`: run discovered health task and convert failures into doctor findings.

## 6) Execution Plan

### Phase 9.1 - Contract and Scaffolding
- [x] Add `doctor` built-in command registration, help text, and base renderer.
- [x] Define finding model (`id`, `severity`, `evidence`, `remediation`, `fixable`).
- [x] Define stable exit behavior and shared report summary shape.
- [x] Add basic JSON schema wiring for `doctor` output (in coordination with roadmap 008 envelope work).

### Phase 9.2 - Slice 1 Checks (Environment + Manifest + Task References)
- [x] Implement environment tool checks from detected task requirements.
- [x] Implement manifest parse checks across discovered catalogs.
- [x] Implement unsupported/unknown TOML key and unsupported value diagnostics with file/key-path evidence.
- [x] Implement duplicate/conflict detection for task surfaces.
- [x] Implement task reference resolution checks.

### Phase 9.3 - Consolidate `repo-pulse` Signal + Health Delegation
- [x] Port useful `repo-pulse` findings into doctor checks (root task surface gaps, parse warnings, health task presence).
- [x] Remove low-value marker chatter and generic root-noise findings.
- [x] Implement implicit `health` task discovery and execution as part of doctor run.
- [x] Ensure delegated health failures preserve command evidence and remediation hints.

### Phase 9.4 - Remediation and `--fix`
- [x] Add safe auto-remediation operations for stale locks and selected deterministic config scaffolds.
- [x] Add dry-run style reporting for skipped/unsafe fixes with reason.
- [x] Ensure `--fix` preserves non-zero exit when unresolved `error` findings remain.

### Phase 9.5 - Command Surface Cleanup
- [x] Remove built-in `repo-pulse` implementation and references.
- [x] Remove built-in `health` alias behavior and fallback paths.
- [x] Update command discovery/help/docs to present only `doctor`.
- [x] Add migration notes for teams/scripts moving from `repo-pulse`/`health` to `doctor`.

### Phase 9.6 - Explain Mode Extension
- [x] Add explain mode as `effigy doctor <task> <args>` for task resolution diagnostics.
- [x] Include candidate catalogs, selection mode/evidence, ambiguity reasoning, and deferral reasoning.
- [x] Add JSON explain payload schema (`effigy.doctor.explain.v1`) with text/JSON parity.

## 7) Acceptance Criteria

- [x] `effigy doctor` is available as the canonical health/remediation built-in.
- [x] `repo-pulse` and built-in `health` paths are removed from command routing and help output.
- [x] Manifest checks report parse failures and unsupported/unknown settings with actionable paths.
- [x] If a `health` task exists, `doctor` executes it and includes results in final findings.
- [x] `doctor --fix` applies only safe remediations and reports each action taken/skipped.
- [x] Exit code semantics are deterministic: non-zero for any `error`, zero for warning/info only.
- [x] JSON mode emits stable, schema-versioned output compatible with roadmap 008 contract direction.
- [x] Explain mode is available via `effigy doctor <task> <args>` in text and JSON modes.

## 8) Risks and Mitigations

- [ ] Risk: removing `repo-pulse`/`health` breaks existing scripts.
  - Mitigation: ship migration notes and clear error messaging with `effigy doctor` replacement examples.
- [ ] Risk: unsupported-key checks generate false positives as schema evolves.
  - Mitigation: centralize manifest key registry and test against fixtures for valid/invalid permutations.
- [ ] Risk: delegated `health` task execution adds noisy failures.
  - Mitigation: classify delegated output under explicit `health.task.execute` findings with bounded evidence excerpts.
- [ ] Risk: `--fix` scope expands unsafely.
  - Mitigation: maintain explicit allowlist of fixers with tests and skip-by-default for ambiguous edits.

## 9) Deliverables

- [x] New built-in `doctor` command with remediation-first finding model.
- [x] Slice 1 checks including unsupported/unknown TOML key/value diagnostics.
- [x] Delegated catalog/root `health` execution integrated into doctor sweep.
- [x] Safe `--fix` baseline and reporting.
- [x] Explain mode extension (`effigy doctor <task> <args>`) with parity across text and JSON output.
- [x] Removal of `repo-pulse` and built-in `health` command surfaces.
- [x] Updated docs, contract tests, and migration guidance.
