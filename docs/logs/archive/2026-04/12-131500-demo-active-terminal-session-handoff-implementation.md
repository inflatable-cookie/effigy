# Demo Active Terminal Session Handoff Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.38`

## Summary

Added a runner-owned active demo terminal/session contract so later browser
surfaces can render live output and terminal metadata without nested TUIs.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `active demo lifecycle state existed, but clients still had no
  dedicated terminal/session handoff contract for live output or nested-TUI
  avoidance` to `the runner now exposes a dedicated active terminal/session
  contract with transport metadata, bounded recent output, and explicit
  no-nested-TUI signaling`
- Remaining open:
  - let the browser consume the active terminal/session contract through a
    bounded terminal view
  - keep multi-process embedding, generic analytics, and desktop-client work
    deferred

## Delivered

- added `active_terminal_session` to `demo inspect` detail output
- added the same terminal/session handoff to `demo run` and `demo stop` JSON
  payloads
- exposed transport kind, pty-vs-stream posture, input-forwarding capability,
  nested-TUI signaling, log references, and bounded recent stdout/stderr lines
- updated help, guides, roadmap/currentness surfaces, and opened the next ready
  browser-view card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The runner now owns the demo terminal handoff boundary. The browser can consume
one demo's live terminal surface next without inventing nested runtime or UI
semantics.

## Next Task

Execute [`045-implement-demo-browser-terminal-view.md`](../../../specs/batch-cards/045-implement-demo-browser-terminal-view.md)
to let `effigy demo browser` consume the active terminal/session contract
through a bounded demo-scoped terminal view.
