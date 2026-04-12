# Demo Post-PTY Terminal Contract Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`052-decide-demo-post-pty-terminal-contract-boundary.md`](../../specs/batch-cards/052-decide-demo-post-pty-terminal-contract-boundary.md)

## Summary

Chose bounded browser terminal convergence as the next slice after PTY-backed
demo terminal/session semantics landed.

## Vision Target Delta

- keep the demo runner contract runner-owned and honest
- let the browser prove it can consume that richer contract live without
  nested-TUI launch
- keep broader tab convergence deferred until after one more bounded browser
  proof step

## Decision

- do not prioritize another runner-only terminal/session batch immediately
- do not prioritize demo-scoped tabs next
- do prioritize one bounded browser live-terminal batch on top of the shipped
  attached-session and PTY-backed contract surfaces
- preserve the no-nested-TUI rule for demos backed by the concurrent runner

## Why

- human CLI interaction is now materially better: attached terminal runs are
  real, and PTY-backed semantics exist where supported
- the next biggest gap is browser-side live consumption of that session, not
  another round of contract expansion
- tab convergence is still plausible, but it is broader presentation churn than
  the lane needs next

## Outcome

Opened ready card [`053-implement-demo-browser-live-terminal-view.md`](../../specs/batch-cards/053-implement-demo-browser-live-terminal-view.md).

## Next Task

Execute [`053-implement-demo-browser-live-terminal-view.md`](../../specs/batch-cards/053-implement-demo-browser-live-terminal-view.md)
to let `effigy demo browser` consume the shipped demo terminal/session contract
as a bounded live terminal view before any tab convergence work.
