# 005 - Unified Testing Orchestration

Status: Complete
Owner: Platform
Created: 2026-02-26
Depends on: 001, 002, 003, 004

## 1) Problem

Effigy can run project-defined test tasks, but teams still have to manually encode common testing conventions in every repo. This creates duplication and uneven defaults across TypeScript and Rust projects, especially in mixed workspace setups.

## 2) Goals

- [x] Add a built-in `effigy test` task with convention-based auto-detection for common ecosystems.
- [x] Prefer `vitest` for TS/JS projects when available.
- [x] Prefer `cargo nextest run` for Rust projects when available, then fall back to `cargo test`.
- [x] Keep explicit project task configuration as the highest-priority override.
- [x] Provide `--plan` mode to show selected commands without executing them.
- [x] Support root + sub-repo resolution and aggregate pass/fail reporting.

## 3) Non-Goals

- [ ] No attempt to normalize every framework-specific test runner in phase 005.
- [ ] No replacement of repo-specific bespoke test orchestration where already defined.
- [ ] No flaky-test retry policy in phase 005.
- [ ] No coverage aggregation/report publishing in phase 005.

## 4) UX Contract

Default invocation:
- `effigy test`
- `effigy test --plan`
- `effigy <repo>/test`

Expected behavior:
- Effigy resolves explicit task config first when present.
- If no explicit test task exists, Effigy applies auto-detection:
  - TS/JS: prefer `vitest` when detected.
  - Rust: prefer `cargo nextest run` when executable exists, else `cargo test`.
- `--plan` prints chosen runner, command, cwd, and fallback path.
- Exit code propagates from executed test command(s) with aggregate summary for multi-target runs.

## 5) Config Model (Target)

Explicit override (wins over auto):
```toml
[tasks.test]
run = "bun test {args}"
```

Fanout concurrency:
```toml
[test]
max_parallel = 2
```

Runner overrides:
```toml
[package_manager]
js = "pnpm"

[test.runners]
vitest = "pnpm exec vitest run"
"cargo-nextest" = "cargo nextest run --workspace"
```

Notes:
- `tasks.test.run` always overrides built-in detection.
- Detection config is optional; built-in defaults apply.
- Detection signals should be explicit and reported in `--plan` output.

## 6) Execution Plan

### Phase 5.1 - Detection Engine Baseline
- [x] Implement single-target runner detection with ranked candidates.
- [x] TS/JS detection: `vitest` config/dependency/bin checks.
- [x] Rust detection: `cargo-nextest` presence check with `cargo test` fallback.
- [x] Add deterministic tie-break rules and reason strings.

### Phase 5.2 - Runner Integration
- [x] Add built-in `test` task route in core runner.
- [x] Wire auto-detected command execution into existing task execution pipeline.
- [x] Preserve argument passthrough (`{args}` behavior) for explicit and auto paths.
- [x] Ensure unresolved built-in detection can still defer when configured.

### Phase 5.3 - Planning and Explainability
- [x] Implement `effigy test --plan` dry-run output.
- [x] Show: target root, selected runner, final command, fallback chain, and detection evidence.
- [x] Add clear error mode when no runner can be detected.

### Phase 5.4 - Multi-Repo Orchestration
- [x] Support workspace-aware fanout for root invocations.
- [x] Execute per-target test commands with bounded parallelism.
- [x] Aggregate result summary by repo with clear non-zero propagation.

### Phase 5.5 - Hardening and Adoption
- [x] Add integration tests for detection + fallback chains.
- [x] Add docs and examples for explicit override vs auto mode.
- [x] Validate on active repos (Acowtancy first) and publish checkpoint report.
- [x] Fix active-repo manifest parse blockers and rerun root fanout validation.

## 7) Acceptance Criteria

- [x] `effigy test` runs meaningful defaults in common TS/JS and Rust repos without extra config.
- [x] `effigy test --plan` explains exactly what would run and why.
- [x] Explicit `tasks.test.run` overrides auto behavior.
- [x] Rust projects use `cargo nextest run` when available and fall back to `cargo test` when not.
- [x] Multi-target summaries are readable and return correct final exit code.

## 8) Risks and Mitigations

- [ ] Risk: false-positive runner detection from stale lockfiles/configs.
  - Mitigation: combine multiple evidence checks and print detection reason.
- [ ] Risk: inconsistent local tool availability (`nextest` installed in some environments only).
  - Mitigation: executable probe with deterministic fallback chain.
- [ ] Risk: noisy behavior across large workspaces.
  - Mitigation: bounded concurrency and concise aggregated reporting.

## 9) Deliverables

- [x] Built-in `effigy test` auto-detection and execution path.
- [x] `--plan` explainability output for test selection.
- [x] Workspace fanout test orchestration and summary.
- [x] Documentation and adoption report.
