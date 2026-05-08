# 070 Implement Demo Browser Live Attached Terminal Session

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Make the browser `Terminal` tab the live terminal session for browser-launched
run-backed interactive demos, so the demo actually runs in that pane instead of
the browser replaying runner logs after the fact.

## In Scope

- add one browser-owned live attached terminal session path for browser-launched
  run-backed demos whose mode requires terminal interaction
- have the `Terminal` tab host the live output and input loop for that session
- preserve runner-owned receipts, logs, latest-attempt state, and retained
  history while the session is live
- keep detached/session-contract surfaces coherent for non-browser clients
- leave concurrent-runner-backed demos on the existing flattened projected path
  for now

## Out Of Scope

- launching or embedding the concurrent TUI inside `effigy demo browser`
- generic process-manager UI or multi-process demo sub-tabs
- redesigning the browser layout or control model again
- broad backend unification beyond the bounded run-backed browser-attached path
- desktop-client work

## Acceptance Criteria

- browser-launched run-backed interactive demos can run directly inside the
  browser `Terminal` tab
- output is live because the demo session is attached there, not because the
  tab is replaying logs
- terminal input goes directly to the live session in that tab
- runner-owned logs, receipts, and history remain populated
- concurrent-runner-backed demos still do not launch nested TUI

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch turns into generic runtime-manager work
- the implementation requires nested TUI launch to stay coherent
- the batch widens into concurrent-runner live embedding instead of the bounded
  run-backed browser-attached path

## Outcome

- browser-launched run-backed interactive demos now run through a browser-owned
  live attached terminal session instead of the terminal tab replaying logs
- the `Terminal` tab now renders live subprocess output and sends typed keys
  directly to that live session while it is active
- the browser still relies on normal `effigy demo run|rerun` subprocesses, so
  runner-owned receipts, logs, latest-attempt state, and retained history stay
  populated without duplicating runner semantics in the TUI
- concurrent-runner-backed demos remain on the existing flattened projected
  path and still do not launch nested TUI

## Next Task

Execute [`071-decide-demo-post-browser-live-attached-terminal-session-boundary.md`](./071-decide-demo-post-browser-live-attached-terminal-session-boundary.md)
to decide the next bounded slice after browser-owned live attached terminal
sessions landed for browser-launched run-backed interactive demos.
