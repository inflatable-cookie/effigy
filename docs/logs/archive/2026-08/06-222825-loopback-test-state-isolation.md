# Loopback Test State Isolation

Status: complete
Created: 2026-08-06
Roadmap: g08.026
Batch: loopback-test-state-isolation

## Summary

- Measured the release-gate flake against a full 50-address registry.
- Confirmed persistent unit-test state leakage, not insufficient pool capacity:
  47 of 50 identities were test-temporary paths.
- Added test-build-only default homes for generated-compose and runner gateway
  state without changing production allocation or the bounded range.
- Added a regression proving runner unit tests cannot fall through to the real
  gateway home.

## Evidence

Before the fix, the focused data-dump test failed with `loopback pool
exhausted`. The new gateway-home regression also failed against the unfixed
fallback and showed the real path.

After both state owners were isolated:

- the focused data-dump and gateway-home regressions pass
- six consecutive `cargo test -q --lib` runs pass: 1,347 tests per run
- the real registry remains unchanged across the valid proof: 1 assignment
  before and 1 after

The first six-run diagnostic happened before runner gateway fallback was
isolated. Those tests pruned 49 stale test-owned assignments from the real
registry, reducing it from 50 to 1. The stale identities were not restored.

A final audit then exercised `cargo test -p effigy-containers`, which does not
inherit the root dev-dependency feature. Its new regression failed by resolving
`/Users/tom/.effigy`, and the audit added seven test assignments before the gap
was fixed. The automatic home now applies under crate-local `cfg(test)` as well
as the cross-crate test-support feature. The direct suite passes all 218 tests
without changing the registry count. Isolation also exposed and corrected a
stale Node mkcert assertion, while absolute test shell paths removed a separate
process-wide `PATH` race. The seven audit-created assignments were removed
afterward; the original non-test assignment remains.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `parallel unit tests allocate and prune persistent user
  loopback state until the release gate flakes` -> current `unit-test state is
  per-process and per-thread; repeated gates leave the user registry unchanged`
- Remaining gap: prepared-source drift policy and full `0.9.1` candidate proof

## Validation Performed

- focused pre-fix data-dump reproduction
  - result: failed with pool exhaustion as expected
- focused pre-fix gateway-home regression
  - result: failed by resolving the real gateway directory as expected
- focused post-fix regressions
  - result: 2 passed
- six repeated `cargo test -q --lib` runs
  - result: 8,082 tests passed; registry count 1 -> 1
- pre-fix crate-local home regression
  - result: failed by resolving the real `/Users/tom/.effigy` as expected
- `cargo test -q -p effigy-containers`
  - result: 218 passed; registry count 8 -> 8
- `cargo clippy --lib --tests -- -D warnings`
  - result: pass
- `cargo fmt --all -- --check`
  - result: pass
- `git diff --check`
  - result: pass

## Boundaries

No production loopback range or allocation behavior changed. No release,
workflow, remote, tag, push, or downstream mutation ran.

## Next Task

Execute card 1068: settle prepared-source drift policy.
