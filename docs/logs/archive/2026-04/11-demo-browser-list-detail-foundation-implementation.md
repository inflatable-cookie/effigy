# Demo Browser List Detail Foundation Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.14`

## Summary

Shipped the first real interactive demo browser on top of the existing demo
registry, inspect, run, stop, rerun, and query/state surfaces.

Delivered in this batch:

- `effigy demo browser` as a TUI entrypoint
- grouped list/detail browsing driven by the shipped demo JSON contracts
- in-browser `run`, `stop`, `rerun`, and refresh actions
- repo-self-hosted proof validation against `browser-proof-report` and
  `lifecycle-window`

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `demo proof requires hopping between multiple CLI commands and
  terminals even after lifecycle control exists` to `one honest first browser
  surface for browsing demos, inspecting proof state, and dispatching bounded
  lifecycle actions`
- Remaining open:
  - decide the next browser follow-up slice between live log visibility and
    artifact-opening affordances
  - defer broader runtime cancellation until generic cancellable handles exist
  - keep desktop-client questions out of the current lane

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- demo browser --group-by status`
- `effigy qa`

## Outcome

Effigy now has an operator-usable browser foundation for demos instead of only
CLI discovery primitives. The new surface is intentionally narrow: it proves
that grouped browsing, detail inspection, and bounded action dispatch are worth
having before the lane widens into logs, artifact affordances, or broader
runtime control.

## Next Task

Use the next `g02.003` ready card to choose whether the browser should add live
log visibility or artifact-opening affordances first.
