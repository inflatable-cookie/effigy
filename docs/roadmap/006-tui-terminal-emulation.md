# 006 - TUI Terminal Emulation

Status: Complete
Owner: Platform
Created: 2026-02-27
Depends on: 004, 005

## 1) Problem

Effigy's TUI currently renders process output using line-oriented heuristics. Tools like `cargo nextest` rely on terminal control sequences (cursor movement, line clearing, rewrite frames), which causes duplicated lines, disappearing lines, and inaccurate scroll ranges.

## 2) Goals

- [x] Replace heuristic line-rewrite handling with a real terminal emulation model for TUI process panes.
- [x] Preserve correct behavior for cursor movement, clears, ANSI styling, and incremental redraw frames.
- [x] Keep process tab UX responsive under high output volume.
- [x] Maintain working process input passthrough for managed processes.
- [x] Keep deterministic process exit reporting and shutdown behavior.

## 3) Non-Goals

- [ ] No full-featured terminal multiplexer scope (splits, session persistence, detached daemon mode).
- [x] No shell redesign scope in this roadmap beyond compatibility with the existing shell tab.
- [ ] No requirement to support every exotic terminal escape extension in phase 006.

## 4) UX Contract

Default behavior:
- `effigy dev`
- `effigy test --tui` (or implicit TUI for multi-suite test fanout)

Expected behavior:
- Process output panes reflect terminal screen state without line duplication or accidental history erasure.
- `nextest`-style progress output leaves stable final lines in scrollback.
- Tab switching, scrolling, options menu, and Ctrl+C quit remain responsive while processes are noisy.
- Exit summary remains ordered and reliable per process.

## 5) Architecture Direction

Target model:
1. Read PTY output as raw bytes.
2. Feed bytes into a VT parser + screen model.
3. Render visible viewport from emulated screen state.
4. Keep bounded scrollback independent from live viewport.

Implementation notes:
- Prefer integrating an existing Rust terminal emulation core rather than expanding ad-hoc ANSI heuristics.
- Keep current line-mode renderer as temporary fallback while emulator mode is validated.

## 6) Execution Plan

### Phase 6.1 - Emulator Spike
- [x] Select emulator core crate and document tradeoffs (integration complexity, feature coverage, maintenance risk).
- [x] Build a single-pane proof of concept: raw PTY bytes -> emulator state -> ratatui render.
- [x] Validate with `cargo nextest run` and one Vite process as baseline fixtures.

### Phase 6.2 - Process Manager Integration
- [x] Add emulator-backed output state per process tab.
- [x] Route PTY event stream to emulator state updates without blocking input handling.
- [x] Keep existing tab, mode, options, and shutdown controls unchanged at UX level.

### Phase 6.3 - Scrollback and Viewport Correctness
- [x] Implement bounded scrollback policy with predictable memory limits.
- [x] Ensure scrollbar range/position reflects true rendered history.
- [x] Preserve copyable, readable historical output after process completion.

### Phase 6.4 - Test Runner TUI Adoption
- [x] Ensure built-in test TUI (`effigy test --tui` and auto-multi-suite TUI) uses emulator-backed panes.
- [x] Validate mixed suite fanout (`vitest` + `nextest`) in one invocation.
- [x] Confirm non-interactive fallback remains text-mode and unchanged.

### Phase 6.5 - Hardening and Rollout
- [x] Add targeted regression coverage for rewrite-heavy streams.
- [x] Add runtime diagnostics toggle for emulator/debug traces (disabled by default).
- [x] Update docs (`guides/012-dev-process-manager-tui.md`, `guides/013-testing-orchestration.md`) with behavior and limitations.

## 7) Acceptance Criteria

- [x] `nextest` output in TUI no longer duplicates or disappears due to control-sequence handling.
- [x] Scrollbar size and range track actual visible/scrollback content.
- [x] Input, tab switching, and quit controls remain responsive during heavy output.
- [x] `effigy test --tui` is stable for multi-suite runs with at least one rewrite-heavy runner.
- [x] Full suite passes with emulator mode enabled by default for TUI panes.

## 8) Risks and Mitigations

- [ ] Risk: emulator integration complexity causes regressions in process manager responsiveness.
  - Mitigation: phase-gate with spike + perf checkpoints before full rollout.
- [ ] Risk: memory growth from unbounded emulator history.
  - Mitigation: strict scrollback caps and periodic trimming policy.
- [ ] Risk: platform-specific PTY nuances (macOS/Linux differences).
  - Mitigation: test matrix across representative local workflows before default enablement.

## 9) Deliverables

- [x] Emulator-backed process output pipeline for TUI tabs.
- [x] Updated TUI rendering logic with accurate viewport/scrollback behavior.
- [x] Regression tests for rewrite-heavy output scenarios.
- [x] Updated TUI/testing guides and checkpoint report.
