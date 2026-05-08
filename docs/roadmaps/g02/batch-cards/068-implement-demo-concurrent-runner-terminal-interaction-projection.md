# 068 Implement Demo Concurrent-Runner Terminal Interaction Projection

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Project bounded terminal interaction for concurrent-runner-backed demos
through the existing demo session contract so browser and CLI consumers can
use input and resize semantics without nested TUI launch.

## In Scope

- add one bounded input-forwarding projection for concurrent-runner-backed
  active demo sessions
- add one bounded resize projection for concurrent-runner-backed active demo
  sessions
- keep the interaction shape demo-scoped and flattened rather than exposing a
  generic process-manager surface
- preserve receipts, logs, history, and the no-nested-TUI rule

## Out Of Scope

- browser layout or control redesign
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process controls
- broader concurrent-runner UX outside the demo contract
- desktop-client work

## Acceptance Criteria

- concurrent-runner-backed active demo sessions report honest input and resize
  availability through the demo session contract
- browser and CLI consumers can use the projected interaction path without
  nested TUI launch
- the contract stays demo-scoped rather than process-manager-scoped

## Result

- concurrent-runner-backed detached demo sessions now expose demo-owned stdin
  and resize handoff paths through the active attempt and active terminal
  session contract
- the managed concurrent runtime now forwards appended input from the demo
  handoff file into one flattened target process when the projected demo
  resolves to a single process
- `demo input` and `demo resize` now work for eligible concurrent-runner-
  backed detached demo sessions through the same demo-scoped contract used by
  browser consumers
- CLI regression coverage now locks inspect, input, resize, and stop behavior
  for concurrent-runner-backed demos

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch requires launching a nested TUI to stay coherent
- the slice widens into generic process-manager interaction controls
- the design cannot stay flattened at the one-demo session boundary

## Next Task

Execute [`069-decide-demo-post-concurrent-runner-terminal-interaction-boundary.md`](./069-decide-demo-post-concurrent-runner-terminal-interaction-boundary.md)
to choose the next bounded slice after concurrent-runner terminal interaction
projection landed.
