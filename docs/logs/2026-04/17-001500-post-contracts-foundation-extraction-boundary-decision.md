# 2026-04-17 001500 - Post Contracts Foundation Extraction Boundary Decision

## Summary

Completed `220` by pausing the contracts seam on an honest runner-shell
boundary.

## Decision

Pause contracts.

After `219`, `src/runner/contracts_command.rs` is down to command entry,
selection print-mode handling, text/json output choice, and runner error
mapping over crate-owned contracts APIs. That is adapter behavior, not another
real `effigy-contracts` extraction target.

## Next Task

Execute
[`221-decide-next-src-shell-cleanup-priority-after-contracts-boundary.md`](../../specs/batch-cards/221-decide-next-src-shell-cleanup-priority-after-contracts-boundary.md)
to choose the next substantial shell cleanup target after the contracts pause
boundary.
