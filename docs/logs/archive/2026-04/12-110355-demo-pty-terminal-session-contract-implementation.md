# Demo PTY Terminal Session Contract Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.45`

## Summary

Deepened the runner-owned demo terminal/session contract with a PTY-backed
attached path for interactive and hybrid run-backed demos on macOS.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `SURFACE`
- Moved from `interactive demo runs attached directly, but terminal-oriented
  demos still looked like split stream processes instead of honest PTY-backed
  sessions` to `interactive and hybrid run-backed demos can now surface a real
  PTY-backed attached session, with active-session inspection and runner-owned
  receipts/logs staying coherent`
- Remaining open:
  - decide whether the next slice should return to browser convergence or keep
    deepening terminal interaction semantics
  - keep nested TUI embedding and broader runtime expansion deferred

## Delivered

- switched attached interactive and hybrid run-backed demo execution to a
  PTY-backed path on macOS
- made active-session inspection report honest `transport=pty` metadata
- kept logs, receipts, and history populated for PTY runs
- treated PTY transcript capture honestly as merged terminal output rather than
  pretending PTY demos still have a clean split stderr stream
- updated help, changelog, roadmap/currentness surfaces, and opened the next
  ready boundary card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The demo runner now has an honest PTY-backed terminal/session contract for
human interactive runs. Browser work can consume that richer contract later
instead of forcing PTY semantics client-side.

## Next Task

Execute [`052-decide-demo-post-pty-terminal-contract-boundary.md`](../../../specs/batch-cards/052-decide-demo-post-pty-terminal-contract-boundary.md)
to choose the next bounded follow-up after PTY-backed demo terminal/session
semantics landed.
