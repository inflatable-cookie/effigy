# Demo Post-Attached-Terminal-Run Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.44`

## Summary

Chose deeper runner-owned PTY terminal/session semantics as the next slice
after attached human terminal runs landed.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `SURFACE`
- Moved from `human text-mode runs now attach directly, but demos that require
  real PTY semantics, terminal capability checks, or richer live interaction
  still depend on a thinner stream-based contract` to `the lane now treats
  PTY-backed runner semantics as the next required contract slice, with browser
  tab convergence still deferred`
- Remaining open:
  - implement bounded PTY-backed demo terminal/session semantics on the runner
  - decide later whether browser tab convergence is still warranted after the
    richer terminal contract lands

## Decision

- do not prioritize browser tab convergence next
- do prioritize deeper runner-owned PTY terminal/session semantics next
- keep demo-browser terminal work demo-scoped and contract-consuming
- keep the no-nested-TUI rule intact for demos backed by the concurrent runner

## Why

- attached streaming solved the human-path baseline but not true terminal
  behavior
- some demos need a real terminal session, not just inherited stdin plus
  mirrored stdout/stderr
- browser work should consume settled terminal semantics, not force them
- tabs are still a presentation choice, not the next contract boundary

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Outcome

The next implementation batch should deepen the runner-owned demo
terminal/session contract with PTY-backed semantics. Browser tab convergence
stays possible later, but it is still not the next honest slice.

## Next Task

Execute [`051-implement-demo-pty-terminal-session-contract.md`](../../specs/batch-cards/051-implement-demo-pty-terminal-session-contract.md)
to deepen the runner-owned demo terminal/session contract with PTY-backed
interactive semantics.
