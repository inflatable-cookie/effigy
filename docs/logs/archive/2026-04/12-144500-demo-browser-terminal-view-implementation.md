# Demo Browser Terminal View Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.39`

## Summary

Added a bounded terminal view inside `effigy demo browser` so the selected demo
can expose active terminal/session state and recent output without leaving the
browser or launching nested TUIs.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `SURFACE`
- Moved from `the runner exposed an active terminal/session contract, but the
  browser still had no in-place terminal view for one selected demo` to `the
  browser now consumes the runner-owned terminal/session contract through a
  demo-scoped terminal detail view`
- Remaining open:
  - decide whether the next bounded slice belongs in browser tab convergence or
    deeper runner-owned terminal input/session work
  - keep nested TUI embedding, multi-process demo tabs, and desktop-client work
    deferred

## Delivered

- added a `View terminal` action in the browser detail pane
- added a bounded terminal detail mode with back/refresh actions
- rendered active terminal metadata, log references, and recent stdout/stderr
  lines from the runner-owned session contract
- rendered unavailable terminal sessions honestly when no active session exists
- updated help, changelog, roadmap/currentness surfaces, and opened the next
  ready boundary card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The browser now has an honest one-demo live terminal view. The next decision
can focus on whether to deepen browser presentation into demo-scoped tabs or go
back down into runner-owned terminal input/session work.

## Next Task

Execute [`046-decide-demo-post-browser-terminal-view-boundary.md`](../../../specs/batch-cards/046-decide-demo-post-browser-terminal-view-boundary.md)
to choose the next bounded follow-up after the shipped browser terminal view.
