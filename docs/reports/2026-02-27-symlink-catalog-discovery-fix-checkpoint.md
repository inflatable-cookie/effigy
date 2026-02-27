# Symlink Catalog Discovery Fix Checkpoint

Date: 2026-02-27
Owner: Platform
Related roadmap: 001 - Effigy Foundation

## Scope

Validate and fix catalog discovery behavior for symlinked subdirectories in workspace roots (acowtancy -> underlay symlink topology).

## Changes

- Updated catalog discovery to traverse symlinked directories and symlinked files when scanning for `effigy.toml`.
- Added canonical-path visited-set dedupe to avoid traversal loops/cycles when symlinks point back into already-scanned directories.
- Added regression test coverage for symlinked catalog discovery and prefixed task resolution.
- Updated docs to document symlink discovery behavior and alias collision caveat.

## Root Cause

Catalog discovery only descended into entries where `file_type.is_dir()` was true. Directory symlinks were excluded, so `acowtancy/underlay -> ../underlay` never got scanned for `effigy.toml`.

## Validation Matrix

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- tasks --repo /Users/betterthanclay/Dev/projects/acowtancy`
  - result (before fix): catalog list excluded `underlay` (6 catalogs total).

- command: `cargo test discover_catalogs_includes_symlinked_catalog_directories -- --nocapture`
  - cwd: `/Users/betterthanclay/Dev/projects/effigy`
  - result: pass (new regression test).

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- tasks --repo /Users/betterthanclay/Dev/projects/acowtancy --task underlay/check:types`
  - result (after fix): pass; `underlay/check:types` discovered from `/Users/betterthanclay/Dev/projects/acowtancy/underlay/effigy.toml`.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- underlay/check:types --repo /Users/betterthanclay/Dev/projects/acowtancy --pretty false`
  - result (after fix): pass; explicit `underlay/...` prefix resolves and executes.

## Risks / Follow-ups

- Symlink discovery increases surface area for alias conflicts where the same or multiple manifests are reachable through different paths.
- Alias uniqueness remains strict and fail-fast; this is documented in README and routing guide.
- If desired, a future enhancement could canonicalize catalog roots when reporting conflicts for even clearer operator diagnostics.

## Next

- Add a focused alias-conflict regression test where two discovered manifests (one via symlink path) intentionally declare the same alias, and assert error text remains actionable.
