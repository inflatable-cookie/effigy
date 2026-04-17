# 045 Implement Demo Browser Terminal View

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Let `effigy demo browser` consume the shipped active demo terminal/session
contract through a bounded demo-scoped terminal view.

## In Scope

- add a browser-side terminal view for the selected demo on top of the
  runner-owned active terminal/session contract
- keep the presentation demo-scoped rather than process-manager-scoped
- render recent live output and terminal metadata without launching nested TUIs
- reflect unavailable terminal sessions honestly when no active session exists

## Out Of Scope

- embedding the concurrent TUI inside `demo browser`
- multi-process demo sub-tabs or generic managed-process tabs
- retained-history replay as an interactive terminal
- full input-forwarding implementation if the runner contract still reports it
  unavailable
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the browser can show a bounded terminal view for the selected demo using the
  active session contract
- the view does not launch nested TUIs
- the browser presentation stays demo-scoped and coherent with the shipped
  `Overview` / `History` / `Artifacts` direction
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into generic process-manager embedding
- the browser requires a nested concurrent TUI to stay coherent
- the change turns into full terminal input plumbing instead of bounded view
  consumption

## Next Task

Execute [`046-decide-demo-post-browser-terminal-view-boundary.md`](./046-decide-demo-post-browser-terminal-view-boundary.md)
to choose whether the next bounded slice should prioritize demo-scoped browser
tab convergence or a deeper runner-owned active-terminal input contract.
