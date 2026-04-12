# Demo Post-Terminal-Resize Contract Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: `063-decide-demo-post-terminal-resize-contract-boundary`

## Summary

After landing browser terminal emulation, input, and runner-owned resize
semantics, the next bounded slice should move back down into the runner rather
than take another browser follow-up.

## Decision

- pause browser terminal work again
- make the next slice bounded runtime backend and capability reporting for
  active demo sessions and inspect surfaces
- keep richer runtimes, including concurrent-runner-backed demos, flattened
  behind one demo-scoped contract
- preserve the no-nested-TUI rule

## Why

- the browser terminal surface is now coherent enough that another immediate
  browser batch would be churn
- the sharper remaining gap is honest backend/capability reporting for richer
  runtimes
- that reporting belongs to the runner contract, not the browser

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved: `post-resize browser-or-runner ambiguity -> explicit runner-owned backend capability contract next`
- Remaining: implement the bounded backend/capability contract in the next card
