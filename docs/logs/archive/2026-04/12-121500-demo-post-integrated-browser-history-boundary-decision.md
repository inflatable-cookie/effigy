# Demo Post-Integrated-Browser-History Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.37`

## Summary

Chose runner-owned active terminal/session handoff as the next slice after the
shipped integrated browser history view.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `the browser can review retained history in-place but still lacks
  a settled answer for live terminal output, terminal input, and concurrent-
  runner-backed demos` to `the next bounded slice is explicitly a runner-owned
  active-session contract that later browser tabs can consume without nested
  TUI launch`
- Remaining open:
  - implement the active demo terminal/session handoff
  - choose how the browser should present `Overview`, `History`, `Terminal`,
    and `Artifacts` once the contract exists
  - keep multi-demo history density, generic analytics, and desktop-client work
    deferred

## Decision

- do not keep widening retained-history activation first; integrated one-demo
  history is enough browser-side consumption for now
- do not launch or embed the concurrent TUI inside `effigy demo browser`; demo
  execution backends must project active terminal behavior through a demo-owned
  session contract
- do treat demo-scoped tabs such as `Overview`, `History`, `Terminal`, and
  `Artifacts` as a plausible browser presentation direction once the runner
  contract exists
- do make the next implementation slice runner-first: one-demo active terminal
  metadata, bounded live-output handoff, and input-forwarding capability
  signaling

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`

## Outcome

The lane stays disciplined while responding to real operator friction. Browser
work does not pretend to own terminal/runtime semantics, and concurrent-runner-
backed demos no longer imply a nested-TUI trap.

## Next Task

Execute [`044-implement-demo-active-terminal-session-handoff.md`](../../../specs/batch-cards/044-implement-demo-active-terminal-session-handoff.md)
to add a runner-owned active demo terminal/session contract before any tabbed
browser terminal integration.
