# 060 Implement Demo Browser Terminal Emulator

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Replace the browser terminal log view with embedded terminal emulation for the
selected demo, including input forwarding where the active runner session
allows it, without launching a nested TUI.

## In Scope

- render the selected demo's active terminal output through embedded terminal
  emulation instead of plain log lines
- forward user input from the browser terminal tab when the runner-owned input
  contract reports it as available
- reuse the existing terminal emulation stack from the concurrent TUI where it
  fits, without importing the concurrent TUI app model
- keep terminal behavior demo-scoped and tied to the selected demo session
- preserve fallback handling for demos with no active session
- update help/tests/docs for the new terminal behavior

## Out Of Scope

- launching or embedding the concurrent TUI inside `effigy demo browser`
- generic multi-process tab UI
- another browser layout/control redesign
- desktop-client work

## Acceptance Criteria

- the browser terminal tab behaves like an embedded terminal surface, not a
  plain log page
- input works when the active session reports it as supported
- demos backed by richer runtimes do not require nested TUI launch
- tests cover rendering and input behavior for the bounded browser terminal
  surface

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts importing the concurrent TUI app model instead of reusing
  terminal primitives
- the implementation requires nested TUI launch to stay coherent
- the work drifts into generic process-manager behavior instead of demo-scoped
  terminal interaction

## Outcome

- shipped a vt-backed terminal replay surface inside the demo-browser
  `Terminal` tab
- added browser-side terminal input capture on top of the runner-owned
  `demo input` surface
- replaced the contract-only `demo input` implementation with a real
  runner-owned active-session input handoff for detached run-backed demos
- preserved fallback handling for demos with no active session and kept the
  no-nested-TUI rule intact
- later operator feedback showed this still was not the full ask because the
  demo was not actually running in that pane

## Next Task

Execute [`061-decide-demo-post-browser-terminal-emulator-boundary.md`](./061-decide-demo-post-browser-terminal-emulator-boundary.md)
to choose the next bounded slice after embedded browser terminal emulation
lands.
