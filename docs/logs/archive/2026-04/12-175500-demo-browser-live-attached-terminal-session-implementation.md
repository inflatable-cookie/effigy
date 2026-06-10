# Demo Browser Live Attached Terminal Session Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`070-implement-demo-browser-live-attached-terminal-session.md`](../../../specs/batch-cards/070-implement-demo-browser-live-attached-terminal-session.md)

## Summary

Replaced browser terminal replay for browser-launched run-backed interactive
demos with a browser-owned live attached terminal session that runs through the
normal `effigy demo run|rerun` path and keeps runner-owned receipts, logs, and
history intact.

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `DEMO`
- Moved: `browser terminal replay/input consumer -> browser-owned live attached terminal session for bounded run-backed browser launches`
- Remaining: decide the next bounded slice after the live attached browser path landed

## Delivered

- browser `Run demo` / `Rerun demo` now branch to a live terminal session for
  run-backed interactive and hybrid demos instead of only starting detached
  background runs
- the `Terminal` tab now renders live subprocess output from that session and
  forwards typed keys directly to it while input capture is enabled
- browser-owned live sessions still execute through the normal `effigy demo
  run|rerun` runner path, so runner-owned logs, receipts, active state, and
  retained history remain authoritative
- concurrent-runner-backed demos stay on the flattened projected session path
  and still do not launch nested TUI
- added focused browser tests around the new live-session branching rule

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

Opened ready card [`071-decide-demo-post-browser-live-attached-terminal-session-boundary.md`](../../../specs/batch-cards/071-decide-demo-post-browser-live-attached-terminal-session-boundary.md).

## Next Task

- Execute `071-decide-demo-post-browser-live-attached-terminal-session-boundary.md`
