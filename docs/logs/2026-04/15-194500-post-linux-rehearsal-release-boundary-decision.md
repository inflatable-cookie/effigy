# Post Linux Rehearsal Release Boundary Decision

Date: 2026-04-15 19:45 Europe/London
Roadmap: `g02.007`

## Summary

The Linux rehearsal proof is strong enough to stop treating local Linux build
validation as the release blocker.

It is not strong enough to move straight into the actual Effigy release batch.

One tighter hardening gap remains:

- the rehearsal Rhai script still re-enters Effigy through
  `cargo run --bin effigy`
- that is redundant and source-checkout-shaped
- Effigy should expose its own built-ins to Rhai through the running process
  instead

## Decision

Do not move into release closure yet.

Open one more bounded hardening batch first:

- generic in-process Rhai dispatch helpers:
  `run_effigy(...)` and `run_effigy_json(...)`
- first typed container helpers for the release/container path
- migrate the Linux rehearsal script off the current subprocess re-entry shape

## Why This Is The Right Boundary

The remaining gap is smaller than the Linux proof question but still belongs in
release hardening, not as a future scripting cleanup:

- release-prep scripting should not require Cargo to call Effigy features
- the Linux rehearsal path is now important enough that its scripting contract
  should be honest before the release is cut
- this is a bounded runtime/API improvement, not another broad release detour

## Vision Target Delta

- Tags: `RELEASE`, `CONTRACT`, `MAINT`
- Moved: `uncertain whether local Linux proof was enough to unblock release
  closure` -> `local Linux proof is good; one tighter Rhai runtime/dispatch
  gap is now the explicit blocker`
- Open: implement in-process Rhai Effigy dispatch and first typed container
  helpers, then reassess whether release closure can proceed directly.

## Next Task

Execute
[`113-implement-rhai-in-process-effigy-dispatch-and-container-helpers.md`](../../specs/batch-cards/113-implement-rhai-in-process-effigy-dispatch-and-container-helpers.md).
