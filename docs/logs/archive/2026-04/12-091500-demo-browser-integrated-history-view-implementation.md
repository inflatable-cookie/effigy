# Demo Browser Integrated History View Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.36`

## Summary

Replaced the browser's history handoff with an integrated retained-history
detail mode so one-demo history review stays inside the TUI.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy could only hand operators out of the browser into the
  dedicated one-demo history surface` to `the browser now consumes the settled
  one-demo history contract in-place through an integrated retained-history
  view with unified detail-pane navigation`
- Remaining open:
  - decide whether any later browser/history follow-up should deepen retained
    attempt activation or return to runner/query work
  - keep `demo list` retained-history density, multi-demo aggregation, and
    generic analytics deferred

## Delivered

- replaced the browser's external `Open history` handoff with an integrated
  `View history` action that switches the detail pane into retained-history
  mode for the selected demo
- made detail-pane `↑` and `↓` navigation cover all visible interactive
  entries rather than only artifacts
- rendered retained attempts and selected-attempt details directly inside the
  browser while still consuming the settled `demo history` JSON contract
- updated help, changelog, roadmap/currentness surfaces, and the active ready
  card state for the next strict-lane decision batch

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The browser no longer breaks operator flow for retained-history review. History
semantics remain runner-owned through the dedicated one-demo contract, while the
client now presents that surface where operators actually need it.

## Next Task

Execute [`043-decide-demo-post-integrated-browser-history-boundary.md`](../../../specs/batch-cards/043-decide-demo-post-integrated-browser-history-boundary.md)
to decide whether the next bounded history/browser follow-up should deepen
browser-side activation from retained attempts or return to runner/query work.
