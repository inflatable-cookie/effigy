# 2026-04-17 02:35:00 BST — Next Src Shell Cleanup Priority After Bootstrap Pause Boundary Decision

## Summary

The next local `/src` priority is CLI help.

Bootstrap is now paused on an honest shell boundary, demo and docs are already
under parallel-thread churn, and `src/cli_help/**` is still fully root-owned
despite the command model and parsing already living in `effigy-cli`.

## Why This Decision

CLI help is the cleanest next seam:

- it is fully disjoint from the active parallel-thread write sets
- it is larger than `process_manager/**` and `src/ui/**`
- it is product-facing CLI contract ownership, not incidental helper code
- it should move cleanly into `effigy-cli` without reopening release or
  container work

`process_manager/**` and `src/ui/**` are still real extraction targets, but
they are better treated as the next subsystem jobs after CLI help rather than
skipping a larger, cleaner CLI-owned seam.

## Decision

- keep `g02.010` active
- make CLI help the next local extraction target
- leave process runtime and UI extraction queued behind it in `g02.017`

## Churn Check

This is still a meaningful architecture move, not just another small runner
diet pass. The remaining value is in shifting root-owned shared surfaces, and
CLI help is the best next example of that.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `ROUTE`
- what moved in this report: local strict-lane focus shifted from `bootstrap
  boundary classification` to `CLI help extraction`
- what remains open: CLI help, process runtime, UI extraction, and the
  remaining heavy runner shells

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`229-implement-effigy-cli-help-extraction.md`](../../specs/batch-cards/229-implement-effigy-cli-help-extraction.md)
to move the root-owned CLI help surface into `effigy-cli`.
