# 2026-04-16 21:25:00 BST — Post Demo Runner Shell Follow Up Cleanup Boundary Decision

## Summary

The demo runner seam can pause now.

After `196`, the remaining `src/runner/demo_command.rs` weight is mostly:
- demo command entry and response wiring
- text/json render flow
- task selection and command bridge routing
- managed runtime supervision
- raw process launch, capture, and stop handling

That is no longer the next `effigy-demo` crate boundary. It is runner/process
shell work.

## Why This Decision

The reusable demo-domain layers are now already crate-owned:
- record and history contracts
- execution-attempt and log shaping
- process helper layer
- managed runtime/backend layer
- runner display/projection layer

What remains is still important, but it is not the next honest `effigy-demo`
extraction target.

## Decision

Pause the demo runner seam on this shell boundary and move to the next `/src`
priority decision.

## Churn Check

Keeping the lane on demo for one more guessed slice would be fake completeness
work. The file is still large, but its remaining mass is now shaped like
runner orchestration rather than reusable demo product API.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: demo runner modularization from mixed domain/shell ownership to an
  honest runner/process boundary
- remaining open: choose the next `/src` shell cleanup priority for `g02.010`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`198-decide-next-src-shell-cleanup-priority-after-demo-boundary.md`](../../../specs/batch-cards/198-decide-next-src-shell-cleanup-priority-after-demo-boundary.md)
to choose the next remaining `/src` seam.
