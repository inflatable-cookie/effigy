# Demo Post-Browser-Terminal-Emulator Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`061-decide-demo-post-browser-terminal-emulator-boundary.md`](../../../specs/batch-cards/061-decide-demo-post-browser-terminal-emulator-boundary.md)

## Summary

Chose runner-owned terminal size and resize semantics as the next bounded slice
after embedded browser terminal emulation landed.

## Vision Target Delta

- move from `browser terminal surface is finally honest but still fixed-size`
  toward `runner-owned demo terminal sessions expose honest size and resize
  semantics`
- keep browser work consuming the runner contract instead of inventing more
  session behavior in presentation code
- remaining gap: implement the bounded resize contract and runtime handoff

## Decision

- do not take another immediate browser layout or control batch
- do deepen runner-owned terminal/session fidelity next
- make active terminal size and resize handoff the next ready implementation
  slice
- preserve the no-nested-TUI rule

## Why

- browser terminal emulation closed the obvious product gap; more browser polish
  now would be churn
- terminal-aware demos still need honest size semantics from the runner
- resize semantics strengthen both attached and browser-consumed terminals
  without widening into generic runtime-manager work

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

Opened ready card [`062-implement-demo-active-terminal-resize-contract.md`](../../../specs/batch-cards/062-implement-demo-active-terminal-resize-contract.md).

## Next Task

Execute [`062-implement-demo-active-terminal-resize-contract.md`](../../../specs/batch-cards/062-implement-demo-active-terminal-resize-contract.md)
to add runner-owned terminal size and resize handoff for active demo sessions.
