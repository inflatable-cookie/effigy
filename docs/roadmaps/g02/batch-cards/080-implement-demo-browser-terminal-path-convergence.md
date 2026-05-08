# 080 Implement Demo Browser Terminal Path Convergence

Status: archived
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Repair the browser live-terminal surface by replacing the browser’s custom
terminal integration path with the same shared session/render path the
concurrent runner already uses, so terminal output fidelity is honest again.

## In Scope

- trace the concurrent-runner live terminal path end to end and identify the
  minimal shared session/render abstraction the browser should consume
- remove or bypass browser-specific terminal parsing/render glue where it
  diverges from the concurrent-runner path
- make browser-launched live terminal sessions use that shared path for:
  - live output ingest
  - vt state updates
  - row shaping/render extraction
  - width/resize fidelity
- preserve the no-nested-TUI rule
- add focused regressions that cover the fidelity failures already seen in
  `lifecycle-window`

## Out Of Scope

- browser chrome polish
- multi-process browser panes or controls
- embedding the concurrent TUI
- broad runner contract redesign outside what convergence strictly needs
- desktop-client work

## Acceptance Criteria

- browser live terminal no longer relies on a separate near-copy of terminal
  integration logic
- browser live terminal output matches the shared concurrent-runner terminal
  path for the same byte stream and width
- `lifecycle-window`-style header and later error output no longer wrap or
  split prematurely in the browser terminal pane
- the lane remains demo-scoped and preserves the no-nested-TUI rule

## Validation

- `cargo test browser_`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch turns into more browser-only symptom patching instead of path
  convergence
- convergence would require launching the concurrent TUI inside the browser
- multiple materially different shared-path designs survive the trace without a
  clear winner

## Next Task

Execute [`081-validate-demo-browser-on-real-project-cohort.md`](./081-validate-demo-browser-on-real-project-cohort.md)
to trial the shipped demo browser and terminal flow on at least two real
consumer projects before release.
