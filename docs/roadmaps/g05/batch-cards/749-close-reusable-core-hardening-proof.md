# 749 - Close Reusable Core Hardening Proof

Roadmap: [`../020-reusable-core-hardening-suite.md`](../020-reusable-core-hardening-suite.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14
Completed: 2026-05-14

## Purpose

Close the reusable-core hardening lane after the bounded cleanup slices land or
are deliberately deferred with evidence.

## Scope

- rerun the focused validation matrix for the landed slices
- record any retained deferrals or residual risks
- refresh front-door currentness surfaces and close lane `083`

## Acceptance

- the reusable-core hardening proof is recorded
- retained deferrals are explicit
- active planning front doors no longer advertise stale ready work

## Result

- reran the lightweight closeout proof for god files and duplicate blocks
- recorded the remaining residual-risk set instead of widening scope
- closed strict lane `083`
- refreshed roadmap front doors so they no longer advertise an active reusable-core ready slice

## Residual Risk

- `src/runner/state_command.rs` remains a warning-level god file at 2150 total
  lines
- `crates/effigy-release/src/lib.rs` remains a warning-level god file at 1622
  total lines
- duplicate-block scan remains at `94` findings with `6` high findings, mostly
  in CLI help topic descriptors plus one container temp-repo helper pair
- provider-package OCI materialization remains deliberately unsupported
- release git helpers remain domain-owned; the process-boundary review did not
  justify widening the shared helper further in this lane

## Validation

- `effigy scan god-files --json`
- `effigy scan duplicate-blocks --json`
- `effigy docs check paths ...`
- `git diff --check`

## Next Task

No active reusable-core hardening slice remains in `g05`.
