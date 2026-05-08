# 028 Implement Demo Browser Detail Navigation

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add bounded detail-pane navigation to `effigy demo browser` so operators can
reach the full selected-demo record once artifacts, receipts, and recent output
push the detail pane beyond one viewport.

## In Scope

- add bounded vertical navigation for the detail pane
- keep artifact selection coherent while the detail pane scrolls
- surface the current detail-position state honestly to the operator
- prove the behavior against the shipped self-hosted demos

## Out Of Scope

- richer live-log streaming
- artifact preview/rendering
- multi-attempt history
- generic runtime cancellation expansion
- desktop-client work

## Acceptance Criteria

- the browser can navigate through long detail content without leaving the TUI
- self-hosted demos with longer artifact/log sections remain usable
- the change stays bounded to detail-pane navigation rather than terminal-like
  scrolling ambitions

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch turns into terminal emulation or free-form viewport management
- the batch reopens richer log rendering or artifact preview scope
- the batch depends on multi-attempt history to feel coherent

## Next Task

Choose the next bounded browser follow-up now that detail-pane navigation is
shipped, and keep deeper runtime or desktop-client work explicitly deferred
unless the new evidence changes that boundary.
