# Rhai In Process Effigy Dispatch And Container Helpers

Date: 2026-04-15 20:15 Europe/London
Roadmap: `g02.007`

## Summary

Closed the remaining runtime hardening gap exposed by the Linux rehearsal
container batch.

Shipped pieces:

- generic in-process Rhai helpers:
  `run_effigy(...)` and `run_effigy_json(...)`
- first typed container helpers:
  `container_up(...)`, `container_down(...)`, `container_shell(...)`
- migration of `release:linux:rehearse` off `cargo run --bin effigy` re-entry
  and onto the running Effigy process

## Real Proof

Ran:

- `cargo test run_manifest_task_run_array_rhai_steps_support_in_process_effigy_dispatch -- --nocapture`
- `cargo test run_manifest_task_run_array_rhai_steps_support_container_helpers -- --nocapture`
- `cargo run --bin effigy -- release:linux:rehearse`

The real rehearsal proof passed again after the migration:

- detached container bring-up still worked
- the Linux binary still built inside the Ubuntu 22.04 rehearsal container
- `smoke:release` still passed
- `distribution check-glibc-floor --max-glibc 2.35` still passed
- the local artifact and proof files were still written under
  `.effigy/linux-release/artifacts/`

## Why This Matters

The Linux rehearsal path is now using Effigy's own scripting/runtime contract
instead of a source-checkout-shaped subprocess workaround.

That makes the release-prep path materially more honest:

- no Cargo assumption just to call Effigy built-ins from Rhai
- no redundant nested Effigy launch for container/distribution/release actions
- a better base for future typed helper widening where string argv glue is too
  awkward

## Vision Target Delta

- Tags: `RELEASE`, `CONTRACT`, `MAINT`
- Moved: `release-prep scripting still depended on cargo-run re-entry into
  Effigy` -> `release-prep scripting can call the running Effigy process
  directly, with first typed helpers for the container path`
- Open: decide whether this closes the last release-prep hardening gap and the
  actual Effigy release batch can now start.

## Next Task

Execute
[`114-decide-post-rhai-dispatch-release-boundary.md`](../../../specs/batch-cards/114-decide-post-rhai-dispatch-release-boundary.md).
