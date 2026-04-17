# 044 Implement Demo Active Terminal Session Handoff

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add a runner-owned active demo terminal/session handoff so later browser
surfaces can render live output and forward bounded terminal input without
launching nested TUIs.

## In Scope

- define a runner-owned active-session payload for one selected demo attempt
- expose whether the active attempt supports terminal input forwarding and
  whether it is PTY-backed or plain stream-backed
- expose bounded live-output references or snapshots in a form the browser can
  consume without inventing process-manager semantics
- keep the contract one-demo and active-attempt scoped rather than multi-demo
  or timeline scoped
- record the no-nested-TUI rule for demos backed by the concurrent runner

## Out Of Scope

- implementing demo-browser tabs or a terminal pane
- replaying retained-history attempts as an interactive terminal
- multi-process demo sub-tabs or generic managed-process embedding inside the
  browser
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the runner exposes a dedicated active demo terminal/session contract that can
  support later `Overview` / `History` / `Terminal` / `Artifacts` browser tabs
- the contract distinguishes active session state from retained terminal
  history and latest-attempt receipts
- demos backed by the concurrent runner are projected through the new demo
  session contract rather than launching nested TUIs
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts implementing tabbed browser UI instead of the runner
  contract
- the contract widens into generic multiprocess/session management outside one
  demo attempt
- the design requires nested TUI launch to stay coherent

## Next Task

Execute [`045-implement-demo-browser-terminal-view.md`](./045-implement-demo-browser-terminal-view.md)
to let the browser consume the active demo terminal/session contract through a
bounded demo-scoped terminal view.
