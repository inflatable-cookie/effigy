# 072 Implement Demo Browser Live Concurrent-Runner Session Parity

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Extend the shipped browser-owned live attached terminal session path to
browser-launched single-process concurrent-runner-backed interactive demos,
while keeping multi-process and nested-TUI shapes on the existing flattened
projected session path.

## In Scope

- add one bounded live-attached browser session path for
  single-process concurrent-runner-backed demos when the backend contract says
  nested TUI is not required
- make the `Terminal` tab host that live session for the bounded backend-parity
  case
- preserve runner-owned receipts, logs, latest-attempt state, stop, input, and
  resize semantics
- keep the projected terminal/session path as the fallback for:
  - multi-process concurrent-runner demos
  - demos whose runtime facts would imply nested TUI or broader process-manager
    semantics
- update roadmap/currentness/help/log surfaces in the same closeout

## Out Of Scope

- generic process-manager tabs or sub-tabs inside `demo browser`
- embedding the concurrent TUI
- redesigning browser layout or controls
- broad backend unification beyond the bounded single-process
  concurrent-runner case
- desktop-client work

## Acceptance Criteria

- browser-launched bounded concurrent-runner demos can run in the browser
  terminal pane as a live attached session
- the no-nested-TUI rule stays explicit and enforced
- multi-process concurrent-runner demos still fall back to the projected
  session path instead of pretending to be live-attached
- the lane closes with one new explicit ready card

## Result

- browser-launched single-process concurrent-runner-backed interactive demos
  now use the browser-owned live attached terminal session path
- multi-process concurrent-runner demos still stay on the flattened projected
  terminal/session surface
- browser live-session selection now follows a bounded backend capability fact
  instead of a run-backed-only check
- attached single-process concurrent-runner text runs now feed stdin through
  the existing demo-owned input handoff so browser live sessions can interact
  honestly without nested TUI launch

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts inventing multi-process demo browser controls
- a coherent implementation would require launching the concurrent TUI
- backend parity for the bounded case cannot be expressed without reopening
  generic process-manager semantics

## Next Task

Execute
`073-decide-demo-post-browser-live-concurrent-runner-session-parity-boundary.md`
to choose the next bounded slice after browser-owned live attached terminal
sessions reached bounded parity for run-backed and single-process
concurrent-runner-backed demos.
