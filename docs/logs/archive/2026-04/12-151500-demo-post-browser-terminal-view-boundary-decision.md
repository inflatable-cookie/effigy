# Demo Post-Browser-Terminal-View Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.40`

## Summary

Chose deeper runner-owned active-terminal input/session contract work as the
next slice after the shipped browser terminal view.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `SURFACE`
- Moved from `the browser could now render one demo's active terminal output,
  but live interaction semantics were still missing and tabs were only a UI
  preference` to `the lane now treats bounded active-terminal input/session
  semantics as the next required contract slice, with browser tab convergence
  deferred until that runner surface is real`
- Remaining open:
  - implement bounded demo-scoped active-terminal input forwarding on the
    runner contract
  - decide later whether browser tab convergence is still warranted after the
    interaction contract lands

## Decision

- do not prioritize browser tab convergence next
- prioritize bounded runner-owned active-terminal input/session semantics next
- keep demo-browser presentation demo-scoped
- keep the no-nested-TUI rule intact for demos backed by the concurrent runner

## Why

- operator feedback says terminal interaction matters for real demo debugging,
  not just terminal viewing
- tabs are presentation sugar until the runner can expose honest interaction
  semantics
- handling input at the runner boundary keeps later browser work from inventing
  transport rules client-side

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

The next implementation batch should deepen the runner-owned active
terminal/session contract. Browser tab convergence stays possible later, but it
is no longer the next honest slice.

## Next Task

Execute [`047-implement-demo-active-terminal-input-contract.md`](../../../specs/batch-cards/047-implement-demo-active-terminal-input-contract.md)
to deepen the runner-owned active demo terminal/session contract with bounded
input-forwarding semantics.
