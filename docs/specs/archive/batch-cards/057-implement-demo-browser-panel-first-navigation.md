# 057 Implement Demo Browser Panel-First Navigation

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Make the converged demo browser controls panel-first: `Tab` moves focus between
the list and detail panels, and arrow keys navigate inside the active panel.

## In Scope

- make `Tab` and `Shift+Tab` switch browser panel focus instead of detail tabs
- keep arrow-key navigation owned by the active panel:
  - list panel: up/down move demos
  - detail panel: left/right switch views and up/down move items inside the
    selected view
- preserve the shipped demo-scoped tabs (`Overview`, `History`, `Terminal`,
  `Artifacts`)
- keep `Esc` hierarchical and non-root-safe
- update help/tests/docs for the new control model

## Out Of Scope

- browser terminal text input or other new interactive terminal controls
- nested TUI embedding or process-manager-shaped sub-tabs
- new runner/query contracts
- desktop-client work

## Acceptance Criteria

- panel focus is explicit and navigable with `Tab` / `Shift+Tab`
- arrow keys navigate within the active panel instead of switching cross-panel
  context implicitly
- demo-scoped tabs still exist, but detail-view selection moves to `←` / `→`
  inside the focused detail panel
- tests cover the new control scheme

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the change starts reshaping browser layout instead of controls
- terminal input starts sneaking back into scope
- the implementation needs new runner contracts to feel coherent

## Next Task

Execute [`058-decide-demo-post-panel-first-navigation-boundary.md`](./058-decide-demo-post-panel-first-navigation-boundary.md)
to choose the next bounded slice after panel-first browser navigation landed.
