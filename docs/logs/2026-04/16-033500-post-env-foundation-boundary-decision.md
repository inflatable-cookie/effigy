# Post-Env Foundation Boundary Decision

Date: 2026-04-16
Owner: Platform

## Summary

`137` is complete.

The remaining env / varlock shell no longer justifies another immediate
`effigy-env` extraction batch. The next reusable domain seam is doctor.

## Decision

Treat the remaining env shell as adapter and runtime policy work:

- runtime-specific schema enablement and `.env` loading policy in
  [`src/runner/env_schema_support.rs`](../../../src/runner/env_schema_support.rs)
- compatibility exports in
  [`src/env_schema.rs`](../../../src/env_schema.rs)

Do not open another env slice by default from that remainder.

Promote doctor as the next modularization target instead:

- manifest schema validation in
  [`src/runner/doctor/manifest/schema.rs`](../../../src/runner/doctor/manifest/schema.rs)
- doctor reference checks in
  [`src/runner/doctor/references.rs`](../../../src/runner/doctor/references.rs)

## Why Doctor Is Next

The env-domain extraction already moved the reusable logic:

- parsing
- resolution
- validation
- secret handling

What remains is smaller and tied to runtime integration.

Doctor still owns a larger reusable product cluster in `runner`:

- manifest schema and section validation
- doctor reference policy
- doctor-specific diagnostics/tests around those contracts

That is the next clearer crate boundary.

## Current State

- active strict lane: `g02.010`
- active ready card: `138`
- queued release card: `115`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `env boundary uncertain after effigy-env extraction`
  to `env boundary classified as mostly adapter/runtime policy with doctor promoted next`
- remains open:
  - doctor foundation extraction
  - later vault-backed rollout through `g02.009`
  - release closure and `v0.3` readiness through `g02.007` once the modularization bar is met

## Next Task

Execute
[`138-implement-effigy-doctor-foundation-extraction.md`](../../specs/batch-cards/138-implement-effigy-doctor-foundation-extraction.md)
to move the first reusable doctor cluster out of `runner`.
