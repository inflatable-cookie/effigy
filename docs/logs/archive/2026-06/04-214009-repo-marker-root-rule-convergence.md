# Repo Marker Root Rule Convergence

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1041`

## Summary

Completed the Effigy repo-marker/root-rule convergence slice.

`effigy-core` now owns stable Effigy marker names and pure filename
predicates. Higher-level crates consume those definitions instead of carrying
parallel copies where production code makes root or manifest decisions.

## Changes

- Added `effigy_core::repo_markers` with:
  - `TASK_MANIFEST_FILE`
  - `LOCAL_OVERLAY_FILE`
  - `ROOT_MARKERS`
  - `task_manifest_path`
  - `has_task_manifest`
  - `is_effigy_config_filename`
- Re-exported `TASK_MANIFEST_FILE` from `effigy-manifest` for crates already
  on the manifest boundary.
- Migrated resolver, runtime discovery, routing, scan, bootstrap, release,
  container policy, container support, bundle, docs, secrets, demo, Rhai,
  distribution, deferral, and workspace provisioning production call sites.
- Kept manifest parsing and composition semantics in `effigy-manifest`.
- Left test fixture literals in place where they make the fixture shape easier
  to read.
- Cleaned three small all-target clippy blockers encountered while validating
  the touched crate set.

## Behavior Preservation

- Root walk-up depth and discovery pruning are unchanged.
- `[manifest].root = true` behavior is unchanged.
- Local overlay discovery remains in manifest composition.
- `effigy.local.toml` gitignore behavior is unchanged.
- No manifest grammar or auto-discovery widening was introduced.

## Validation

- `cargo fmt --all`
- `cargo test -p effigy-core`
- `cargo test -p effigy-routing`
- `cargo test -p effigy-runtime read::discovery -- --nocapture`
- `cargo test command_context -- --nocapture`
- `cargo clippy -p effigy-core -p effigy-manifest -p effigy-runtime -p effigy-routing -p effigy-bootstrap -p effigy-release -p effigy-scan -p effigy-builtin -p effigy-containers -p effigy-distribution -p effigy-demo -p effigy-rhai --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy test --plan`
- `git diff --check`

## Vision Target Delta

- Tags: `MAINT`, `CONTRACT`
- Baseline: stable Effigy marker names and repo-marker predicates were copied
  across routing, runtime, manifest-adjacent, and runner code.
- Current: stable marker names and pure predicates have one low-level owner in
  `effigy-core`, with manifest re-export support for existing boundaries.
- Remaining open: selected duplicate-block follow-through in `g08.009`.

## Next Task

Run ready card `1042`.
