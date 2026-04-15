# Post Rhai Dispatch Release Boundary Decision

Date: 2026-04-15 20:35 Europe/London
Roadmap: `g02.007`

## Summary

The release-prep hardening work is now complete enough.

The Linux rehearsal path is real, and the Rhai runtime no longer relies on
`cargo run --bin effigy` re-entry to call Effigy features.

That means the next valid move is the actual Effigy release-closure batch.

## Decision

Do not open another hardening detour.

Move straight into release closure:

- prepare the release-facing state around the shipped optional distribution
  boundary
- include the new Linux rehearsal support honestly in that closure
- stop short of irreversible release execution unless explicitly requested

## Why This Is The Right Boundary

The remaining work is now release work, not runtime work:

- the local Linux proof exists
- the scripting/runtime contract for that proof is now honest enough
- another pre-release API cleanup batch would be churn rather than risk
  reduction

The larger modularization/crate-boundary architecture question is real, but it
belongs in its own later lane rather than delaying this release closure again.

## Vision Target Delta

- Tags: `RELEASE`, `MAINT`, `CONTRACT`
- Moved: `actual release batch still blocked by runtime hardening` ->
  `runtime hardening is sufficient; release closure can now proceed`
- Open: execute the actual Effigy release-closure batch.

## Next Task

Execute
[`115-implement-effigy-distribution-release-closure.md`](../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md).
