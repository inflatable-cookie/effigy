# 1067 - Remove Loopback Test State Leakage

Roadmap: [`../026-patch-release-lane-hardening.md`](../026-patch-release-lane-hardening.md)
Contract: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md)

Status: Complete
Owner: Platform
Created: 2026-08-06
Ready after: operator-selected g08 scope

## Purpose

Restore a stable `cargo test` release gate by preventing generated-compose unit
tests from allocating persistent loopback identities in the user's real Effigy
home.

## Owner And Seam

The runner/container test boundary owns this card. Production loopback range,
allocation semantics, pruning, and user state remain unchanged.

## Measured Baseline

- the real registry contains 50 assignments across 50 unique IPs
- 47 identities are test-temporary paths
- the focused failing test fails against that full registry
- the recent runner gateway-home guard does not redirect the separate
  `effigy-containers` generated-compose home resolver

## Work

- add a non-global test override usable by runner unit tests when generated
  compose resolves Effigy home
- apply it to every runner test path that can generate compose state
- retain the existing runner gateway-home isolation where gateway state is
  also accessed
- add focused proof that the failing path does not touch persistent state
- make crate-local `effigy-containers` tests use the same automatic isolated
  home even when Cargo does not enable the root test-support feature
- repeat the library suite and compare real registry counts before and after

## Acceptance

- [x] the focused test passes with the real registry already full
- [x] no runner generated-compose unit test writes to the real Effigy home
- [x] at least six repeated `--lib` runs pass
- [x] direct `cargo test -p effigy-containers` passes without registry growth
- [x] the real registry assignment count is unchanged after validation
- [x] no production loopback range or allocation behavior changes

## Validation

- focused regression test against the pre-fix failure condition
- six repeated `cargo test --lib` runs with a pre-built Effigy binary
- direct `cargo test -p effigy-containers`
- affected test selection from the graph
- formatting and focused Clippy
- real registry count before/after, read-only

## Stop Conditions

Stop if the fix requires process-global environment mutation, a wider loopback
pool, deletion of user state, or production allocation changes.

## Next Task

Execute card 1068 and settle prepared-source drift policy.
