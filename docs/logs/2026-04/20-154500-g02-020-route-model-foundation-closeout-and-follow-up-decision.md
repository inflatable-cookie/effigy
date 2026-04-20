# g02.020 Route-Model Foundation Closeout And Follow-Up Decision

Date: 2026-04-20
Roadmap: `g02.020`

## Summary

`301` is now closed in file state.

The `dns_ip` route-model foundation landed, the strict-lane surfaces no longer
advertise it as staged work, and the next bounded `g02.020` move is now
explicit: loopback-IP allocation before HTTP post-start published-port
discovery.

## What Changed

- marked `301` as landed with an honest result and next-task pointer
- added `302` to capture the post-`301` boundary call
- added staged card `303` for loopback-IP allocation and gateway setup
  integration
- refreshed the `g02.020` strict-lane, roadmap, and batch-card front doors so
  they point at `303` instead of the pre-foundation state

## Current State

- the `g02.020` route-model seam is now real in code and tests
- the next bounded execution slice for that lane is loopback-IP allocation on
  the bounded macOS path
- repo front doors still keep `g02.007` as the live lane and `g02.019` as the
  next queued audit/documentation lane

## Boundary Call

Loopback-IP allocation comes before HTTP post-start published-port discovery.

That order keeps the lane pointed at the bigger remaining product gap first:
stable TCP service identity and isolation across many simultaneous projects.

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Finish the deliberate `g02.007` release-prep lane, then land `g02.019`.

After those fronts settle, resume `g02.020` at `303`.
