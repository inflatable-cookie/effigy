# 062 Implement Demo Active Terminal Resize Contract

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add runner-owned terminal size and resize handoff for active demo sessions so
terminal-aware demos can react honestly in attached and browser-consumed
surfaces without launching a nested TUI.

## In Scope

- extend the active demo terminal/session contract with bounded terminal size
  metadata and resize capability signaling
- add a runner-owned resize command or equivalent handoff surface for active
  demo sessions where the runtime can support it
- wire the current detached/browser-consumed demo runtime through that resize
  handoff where honest
- keep browser terminal consumption contract-driven rather than inventing
  browser-local session semantics
- update help/tests/docs for the new resize surface

## Out Of Scope

- another browser layout or control redesign
- generic multi-process runtime controls
- embedding the concurrent TUI inside `effigy demo browser`
- desktop-client work

## Acceptance Criteria

- active demo terminal/session state reports terminal size and resize posture
- the runtime exposes one bounded resize handoff where supported
- the batch stays demo-scoped and preserves the no-nested-TUI rule
- tests cover the new contract and runtime behavior

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts importing the concurrent TUI app model instead of reusing
  terminal/session primitives
- the work drifts into generic runtime-manager semantics
- the runtime cannot expose honest resize behavior without a broader boundary
  decision

## Result

- added `effigy demo resize <DEMO_ID> --cols <COLS> --rows <ROWS>`
- extended the active terminal/session contract with terminal size metadata,
  resize posture, resize command metadata, and resize handoff paths
- wired detached/browser-consumed demo sessions through a runner-owned resize
  handoff surface and had the browser terminal tab report viewport changes
  through it when available
- updated help, tests, changelog, and currentness surfaces around the new
  terminal resize contract

## Next Task

Execute [`063-decide-demo-post-terminal-resize-contract-boundary.md`](./063-decide-demo-post-terminal-resize-contract-boundary.md)
to choose the next bounded slice after active demo terminal resize semantics
landed.
