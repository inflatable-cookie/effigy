# TUI Terminal Emulation Phase 6.1 Spike

Date: 2026-02-27
Owner: Platform
Related roadmap: 006 - TUI Terminal Emulation

## Scope
- Implement a first emulator-backed output path for TUI process tabs.
- Preserve existing line-based event consumers while introducing raw chunk events.
- Keep core test suite green after integration.

## Changes
- Added raw stream event kinds in process manager:
  - `StdoutChunk`
  - `StderrChunk`
- Extended `ProcessEvent` with optional byte payload (`chunk`) to carry raw PTY data.
- Updated stream reader threads to:
  - emit raw chunk events for every read,
  - continue emitting line events for compatibility.
- Added vt100-backed output path in TUI:
  - new parser per process tab (`vt100`),
  - chunk events feed parser state,
  - active output pane can render from emulated screen contents.
- Added feature toggle:
  - `EFFIGY_TUI_VT100=0|false` disables vt100 path and keeps existing line-mode rendering.
  - default behavior uses vt100 path when chunk events are present.
- Updated managed stream-mode runner and process manager tests to handle the new chunk event kinds.

## Validation
- command: `cargo test -q`
  - result: pass (full suite)

## Risks / Follow-ups
- Current spike renders screen contents text-first (not full cell-style fidelity yet).
- Manual live validation against real `nextest` + `vite` workflows is still pending.
- Long-term direction remains full terminal-screen rendering with explicit viewport/scrollback policy.

## Next
- Run manual validation in real projects:
  - `effigy test --tui` with nextest-heavy output,
  - `effigy dev` with at least one Vite process,
  - capture any remaining rendering drift before phase 6.2 integration hardening.
