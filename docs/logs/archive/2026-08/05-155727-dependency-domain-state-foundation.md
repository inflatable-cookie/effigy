# Dependency Domain State Foundation

Status: complete
Created: 2026-08-05
Roadmap: `g08.019`
Batch: `1051`

## Summary

Added the shared `effigy-deps` domain and state foundation required before
Cargo/Bun inventory, CLI wiring, or manager mutation.

## Changes

- added `effigy-deps` below CLI, doctor, and runner shells
- added typed manager, link, source, package, plan, drift, verification, and
  report models
- added deterministic repo state at
  `.effigy/local/dependency-links.json`
- added locked machine state at
  `~/.effigy/deps/bun-registrations.json`
- used advisory file locking so stale lock files are reusable while live
  contention fails immediately
- preserved foreign Bun registration ownership and shared consumer references
- added read-only `.effigy/` ignore-coverage planning
- updated the live package map

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: dependency-link state existed only as a contract -> a typed,
  deterministic, shared persistence seam now exists below command and doctor
  shells
- Remaining gap: read-only manager inventory/status, CLI/JSON wiring, manager
  mutation, doctor hygiene, and portfolio proof remain in `g08.019` through
  `g08.023`

## Validation Performed

- `cargo test -p effigy-deps`
  - result: 11 tests and doc tests passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: passed
- `cargo check -p effigy`
  - result: passed
- `cargo tree -p effigy-deps --depth 1`
  - result: only `fs2`, Serde, and test-only `tempfile`; no upward Effigy
    dependencies
- `cargo fmt --all -- --check`
  - result: passed
- `effigy qa:docs`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- the state models deliberately do not invoke Cargo, Bun, Git, or doctor yet
- Bun registration removal still requires later validation of other consumer
  references before physical unregister

## Next Task

Execute ready batch card `1052`.
