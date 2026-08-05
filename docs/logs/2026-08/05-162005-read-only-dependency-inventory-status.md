# Read-Only Dependency Inventory And Status

Status: complete
Created: 2026-08-05
Roadmap: `g08.019`
Batch: `1052`

## Summary

Added deterministic, read-only Cargo and Bun inventory plus manager-neutral
dependency-link status below the future CLI and doctor surfaces.

## Changes

- added an injected read-only process port with actionable spawn/exit failures
- inventoried Cargo workspaces, nested consumer workspaces, and workspace-less
  multi-crate libraries through Cargo metadata
- recovered exact declared Cargo git URLs from manifests and kept git, path,
  registry, and unmatched name collisions distinct
- classified full matching Cargo and Bun closures as direct or transitive
- inventoried Bun root and declared workspace packages and normalized
  `bun pm ls --all` behind the process boundary
- inspected desired state against Cargo managed blocks/resolution, Bun
  registration ownership, and consumer symlink targets without writes
- reported missing paths, full link loss, partial closure, unmanaged local
  resolution, malformed/orphan Cargo blocks, and Bun index conflicts

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`
- Movement: dependency-link desired state existed without manager evidence ->
  Cargo/Bun inventories and physical drift are now typed and inspectable
- Remaining gap: CLI/JSON exposure, manager mutation, doctor hygiene, and
  portfolio proof remain in `g08.019` through `g08.023`

## Validation Performed

- `cargo test -p effigy-deps`
  - result: 24 tests and doc tests passed
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
  - result: passed
- `cargo fmt --all -- --check`
  - result: passed
- `effigy qa:docs`
  - result: passed
- `git diff --check`
  - result: passed

## Risks

- Bun currently offers no JSON form of `pm ls`; the text parser is isolated
  behind the injected adapter and needs supported-version proof in `g08.021`
- inventory and status are read-only; link/unlink mutation remains deliberately
  unavailable

## Next Task

Execute ready batch card `1053`.
