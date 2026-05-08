# 164 Decide Post Demo Browser Host Runtime Loop Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/tui/demo_browser.rs` shell is now honest
adapter work after `163`, and use that decision to pick the next real `/src`
modularization seam instead of drifting on the browser path.

## In Scope

- inspect the remaining root browser shell after `163`
- decide whether the browser seam can pause
- assess the next largest still-dirty `/src` seam honestly
- update lane state and next-task surfaces

## Out Of Scope

- implementation work beyond the decision itself
- release-lane execution
- unrelated cleanup outside the active modularization lane

## Decision

The demo-browser seam can now pause.

`src/tui/demo_browser.rs` no longer owns the browser app loop or the browser
host/runtime contract. The remaining production shell is now just:

- `run_demo_browser_tui(...)`
- `invoke_demo_json(...)`

The rest of the file is browser-focused tests. That is clean enough to stop
extracting from this seam for now.

`g02.010` does not pause, though. The broader `/src` churn check still shows
two major runner-heavy seams:

- `src/runner/release_command.rs`
- `src/runner/demo_command.rs`

The next honest move is `demo_command.rs`, because `effigy-demo` is already
real and this file still owns a large runtime/persistence/render cluster that
is more clearly domain logic than the remaining release shell.

## Next Task

Execute
`165-implement-effigy-demo-runner-runtime-and-persistence-follow-up-extraction.md`
to extract the next meaningful `effigy-demo` runner slice from
`src/runner/demo_command.rs`.
