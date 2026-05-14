# Schema Shape Convergence Closeout

Date: 2026-05-14

## Summary

Completed `g05.018` and `g05.019`.

## Changes

- added manifest-owned `ManifestTaskLikeDefinition`
- switched `[tasks]` parsing to the canonical task-like owner
- collapsed `ManifestBootstrapRun` into a transparent wrapper around the
  canonical task-like owner
- added manifest-owned `ManifestTaskOrReferenceDefinition`
- replaced runner-private state task-like parsing with the manifest-owned
  reference-or-inline owner
- preserved state inline hook/capture host default policy outside the generic
  manifest parser
- fixed bundle user-config lookup so underscore and hyphen bundle ids resolve
  the same configured block
- marked `g05.016`, `g05.018`, and `g05.019` complete
- refreshed g05 front doors to point at the reusable-core hardening tranche

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Baseline: task-like TOML shapes were split across `[tasks]`, bootstrap, and
  state hook/capture parsing.
- Current state: reusable task-like building blocks live in `effigy-manifest`;
  state keeps only execution-policy projection for host defaults.

## Validation

- `cargo test -p effigy-manifest single_task_object_without_array_wrapper`
- `cargo test -p effigy-manifest bootstrap_run_accepts_compact_inline_task_run_in`
- `cargo test --lib compact_inline_task_run_in`
- `cargo test -p effigy-manifest -- --test-threads=1`

## Next Task

Open the reusable-core hardening suite when ready, starting at `g05.020` or
`g05.021`.
