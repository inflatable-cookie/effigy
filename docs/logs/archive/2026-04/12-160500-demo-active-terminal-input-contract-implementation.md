# Demo Active Terminal Input Contract Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.41`

## Summary

Added a bounded runner-owned demo terminal input contract and matching
`effigy demo input` command surface so later browser work has one honest
demo-scoped forwarding shape to consume.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved from `the runner exposed one demo's active terminal session for viewing,
  but had no explicit forwarding surface for later live interaction` to `the
  runner now exposes a bounded demo-scoped terminal input contract and command
  shape alongside the active terminal session`
- Remaining open:
  - let the browser terminal view consume the shipped input contract through one
    bounded interaction affordance
  - keep tabs, nested TUI embedding, and broader runtime expansion deferred

## Delivered

- added `effigy demo input <DEMO_ID> --text <TEXT> [--append-newline]`
- added explicit `active_terminal_session.input_forwarding` contract metadata
- kept the runtime honest: current demos still report forwarding unsupported
  unless the active runtime exposes it
- updated help, changelog, roadmap/currentness surfaces, and opened the next
  ready browser affordance card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The runner now owns one bounded demo input surface. Later browser work can
consume that contract directly instead of inventing terminal-input transport
rules client-side.

## Next Task

Execute [`048-implement-demo-browser-terminal-input-affordance.md`](../../../specs/batch-cards/048-implement-demo-browser-terminal-input-affordance.md)
to let the browser terminal view consume the shipped runner-owned input
contract through one bounded interaction affordance.
