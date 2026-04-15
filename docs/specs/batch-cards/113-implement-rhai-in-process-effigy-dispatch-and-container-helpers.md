# 113 Implement Rhai In Process Effigy Dispatch And Container Helpers

Status: complete
Updated: 2026-04-15
Roadmap: `g02.007`
Spec: `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`

## Objective

Close the remaining release-hardening gap by letting Rhai scripts call Effigy
surfaces in-process instead of shelling back through `cargo run --bin effigy`.

## In Scope

- add generic Rhai host helpers for in-process Effigy dispatch:
  `run_effigy(...)` and `run_effigy_json(...)`
- add first typed container helpers where scripts should not build argv by hand
- migrate `release:linux:rehearse` off the current subprocess re-entry pattern
- document the new Rhai host contract where release/container scripting relies
  on it

## Out Of Scope

- the actual Effigy release
- broad typed helper coverage for every built-in surface
- consumer rollout work

## Acceptance Criteria

- Rhai scripts can invoke Effigy features through the running binary without
  assuming Cargo or a source checkout
- the Linux rehearsal script no longer shells out through
  `cargo run --bin effigy`
- the first typed container helpers exist for the release/container path
- the lane can then decide release closure from a cleaner scripting boundary

## Validation

- targeted tests for the new Rhai host API
- real rerun of `release:linux:rehearse`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `114-decide-post-rhai-dispatch-release-boundary.md` to decide whether
the Rhai dispatch hardening closes the last release-prep gap or still leaves
one tighter release boundary card.
