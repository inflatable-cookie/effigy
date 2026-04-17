# 2026-04-17 02:20:00 BST — Post Bootstrap Integration Test Ownership Boundary Decision

## Summary

Bootstrap now pauses cleanly.

After `226`, `src/runner/bootstrap_command.rs` is down to `402` lines — `87`
lines of shell plus `~316` lines of runner-path integration tests. The shell
itself owns nothing but:

- CLI entry (`run_bootstrap` → `run_bootstrap_with_cwd`)
- callback wiring that adapts `load_task_manifest` and
  `run_manifest_task_with_cwd` to the crate's closure API
- pass-through `resolve` + `execute` that only do error mapping
- the final plan-vs-execute render choice
- `BootstrapError` → `RunnerError` translation

All domain behavior — repo sync, manifest parsing, submodule policy,
children orchestration, task dispatch, plan/result rendering — lives in
`effigy-bootstrap`. Crate-domain tests live with the crate. Runner-path tests
test the runner's actual wiring.

## Why This Decision

The shell is adapter-shaped. Any further extraction would push genuine runner
concerns (task-invocation machinery, `RunnerError` mapping, CLI arg handling)
into the crate and break the boundary. That is fake completeness work.

## Decision

- pause bootstrap on the current boundary
- keep `effigy-bootstrap` as the owner of bootstrap request/execution/render
  semantics
- move the active lane to the next priority survey

## Churn Check

This is a real boundary, not polish. The runner module lost `401` lines across
`224` + `226` (plan/result rendering + crate-domain tests), and what remains is
the minimum viable adapter surface.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: bootstrap now paused on an honest shell boundary with crate-domain
  ownership fully aligned
- remaining open: pick the next `/src` cleanup priority, or pause the lane if
  no eligible disjoint seam remains

## Validation

- `cargo test` — full workspace green
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

Execute
[`228-decide-next-src-shell-cleanup-priority-after-bootstrap-pause-boundary.md`](../../specs/batch-cards/228-decide-next-src-shell-cleanup-priority-after-bootstrap-pause-boundary.md)
to pick the next `/src` cleanup priority after pausing bootstrap.
