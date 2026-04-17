# g01 Reconciliation And Research Carry-Forward

Date: 2026-04-17
Roadmap: `g01`, `g02.018`

## Summary

The stale `g01` layer is narrower now.

This batch closed the obviously satisfied `g01` roadmaps, carried the
unfinished research residue from `021` and `022` into one new `g02` roadmap,
and left the real pre-`v0.3` blockers exposed instead of buried under old
research/program drift.

## What Closed

- `g01.017` comment-ratio scan
- `g01.020` research phase 1
- `g01.021` research phase 2
- `g01.022` research phase 3
- `g01.024` release pipeline validation and consumer CI
- `g01.029` Northstar + Effigy consumer adoption kit

## What Moved

The unfinished research residue from `021` and `022` now lives in:

- `g02.018` research promotion and carry-forward

That roadmap is explicitly non-blocking for `v0.3`.

## What Still Matters Before v0.3

The real remaining blockers are now clearer:

- `g02.010` remaining `/src` cleanup and final reconciliation
- `g01.023` builtin test suite lifecycle/env
- `g01.027` live built-in release closeout

## Vision Target Delta

- primary vision tags touched: `MAINT`, `RELEASE`, `CONTRACT`
- moved from `stale g01 roadmap noise obscuring the real ship blockers` to
  `closed historical items plus one explicit g02 carry-forward roadmap`
- remains open: finish `010`, complete `023`, complete `027`, then reassess
  release execution
