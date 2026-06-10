# Demo Browser Terminal Emulator Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`060-implement-demo-browser-terminal-emulator.md`](../../../specs/batch-cards/060-implement-demo-browser-terminal-emulator.md)

## Summary

Shipped embedded terminal emulation and browser-side terminal input capture in
`effigy demo browser`, backed by a real runner-owned input handoff instead of a
log-only terminal page.

## Vision Target Delta

- move from `browser terminal tab is a text/log summary` toward `browser
  terminal tab is a real demo-scoped terminal surface with input where the
  active session allows it`
- keep terminal behavior runner-owned and demo-scoped instead of launching a
  nested concurrent TUI
- remaining gap: decide whether the next follow-up belongs in deeper
  runner-owned terminal fidelity or whether browser work can pause again

## Delivered

- replaced the browser terminal tab's log page with embedded terminal-emulator
  rendering backed by the selected demo's active or latest-attempt logs
- added browser-side terminal input capture so `Enter` toggles input mode on
  the terminal tab and typed keys forward through `demo input` when the active
  session reports support
- made `demo input` real by appending to a runner-owned active-session handoff
  file for detached run-backed demos instead of only advertising a contract
- preserved latest-attempt fallback when no active session exists and kept the
  no-nested-TUI rule intact
- expanded browser and runner tests around terminal rendering, scroll/input
  mode, key mapping, and input-handoff writing

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

Opened ready card [`061-decide-demo-post-browser-terminal-emulator-boundary.md`](../../../specs/batch-cards/061-decide-demo-post-browser-terminal-emulator-boundary.md).

## Next Task

Execute [`061-decide-demo-post-browser-terminal-emulator-boundary.md`](../../../specs/batch-cards/061-decide-demo-post-browser-terminal-emulator-boundary.md)
to choose the next bounded slice after embedded browser terminal emulation
landed.
