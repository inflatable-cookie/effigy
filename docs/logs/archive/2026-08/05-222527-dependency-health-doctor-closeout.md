# Dependency Health Doctor Closeout

Status: complete
Created: 2026-08-05
Roadmap: g08.022
Batch: 1061-integrate-dependency-health-with-doctor-and-closeout

## Summary

- adapted the shared `effigy-deps` status report into doctor findings without
  adding Cargo or Bun classification to doctor
- exposed healthy links as visible information, Bun full loss as warning, and
  shared conflict/do-not-commit/duplicate-peer findings as errors
- preserved manager, mechanism, library, consumer roots, package, exact
  evidence, and remediation in doctor text and JSON
- closed `g08.022` and opened the `g08.023` runway through cards `1062` to
  `1064`

## Changes

- added the `dependencies.link-health` doctor check and a pure report adapter
- wired doctor to the read-only dependency inspector; Cargo resolution uses
  shared `cargo metadata` observation, while no link, install, unlink, or fix
  mutation is available from doctor
- kept ordinary informational doctor findings hidden while making healthy
  dependency links explicitly renderable
- added adapter severity/context fixtures, healthy-info rendering proof,
  doctor text/JSON parity, and an end-to-end orphan Cargo block contract test
- updated the changelog and strict-lane/front-door currentness

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: dependency drift was visible only through `effigy deps status` ->
  doctor now projects the same typed observations and exact repair evidence
- Remaining gap: real Signal consumer proof, portfolio-shaped Bun drift proof,
  and operator guidance remain in `g08.023`

## Validation Performed

- command: `cargo test -p effigy-doctor`
  - result: 56 passed
- command: `cargo test --lib doctor_json_contract_surfaces_orphan_dependency_link_state`
  - result: 1 passed
- command: `cargo clippy -p effigy-doctor --all-targets -- -D warnings`
  - result: passed
- command: `cargo run --quiet --bin effigy -- doctor`
  - result: 17 ok, 0 warnings, 0 errors
- command: `effigy qa:ci:fast`
  - result: 1,625 tests passed; released-surface and 25 JSON contracts passed
- command: `effigy qa:docs`
  - result: links, examples, indexes, policy, workflow paths, and next-action
    checks passed
- command: `git diff --check`
  - result: passed

## Risks

- doctor performs read-only `cargo metadata` for active Cargo links because the
  shared contract requires resolved-source proof; it performs no manager
  mutation
- real portfolio repositories remain untouched; their proof is isolated to
  disposable clones in card `1062`

## Next Task

- Execute ready card `1062` and prove Signal links against disposable
  Soundcheck and Loophole clones.
