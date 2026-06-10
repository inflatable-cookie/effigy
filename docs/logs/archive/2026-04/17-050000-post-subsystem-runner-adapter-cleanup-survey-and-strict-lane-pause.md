# 2026-04-17 05:00:00 BST — Post Subsystem Runner Adapter Cleanup Survey And Strict Lane Pause

## Summary

The `g02.010` strict modularization lane pauses on a clean full boundary.

`g02.017` queue job #8 (post-subsystem runner adapter cleanup) reran the
`/src` churn check now that `effigy-process` (job #4) and `effigy-ui`
(job #6) have both settled. The survey found no adapter residue exposed by
the subsystem moves.

## What Was Surveyed

All runner shells and the root `cli_help.rs`, grouped by status:

**Under parallel-thread churn (off-limits, owned by the container/demo/docs thread):**
- `src/runner/demo_command.rs` (2819)
- `src/runner/docs_command.rs` (977)
- `src/runner/container_command.rs` (790)

**Paused on honest adapter boundaries:**
- `src/runner/release_command.rs` (1253) — git-facing execute, verify-install
  temp-fixture orchestration, interactive text review
- `src/runner/distribution_command.rs` (658) — metadata validation,
  preflight orchestration, GLIBC floor inspection
- `src/runner/bootstrap_command.rs` (89) — CLI entry, callback wiring,
  error mapping
- `src/runner/contracts_command.rs` (157) — pure CLI→crate dispatch
- `src/cli_help.rs` (187) — HelpRenderer orphan bridge + CLI header
  theming

**Small honest adapters (already adapter-shaped):**
- `src/runner/changelog_command.rs` (200) — CLI→crate dispatch
- `src/runner/script_command.rs` (167) — CLI→crate dispatch
- `src/runner/render.rs` (119) — output rendering adapter
- `src/runner/error.rs` (230) — `RunnerError` enum + `From` impls for each
  subsystem error
- `src/runner/mod.rs` (129) — dispatch table

## Survey Finding

Subsystem references in paused/adapter shells, after the process + UI moves:

| file | effigy-process refs | effigy-ui refs |
|------|---------------------|----------------|
| bootstrap_command.rs | 0 | 0 |
| contracts_command.rs | 0 | 0 |
| changelog_command.rs | 0 | 0 |
| script_command.rs | 0 | 0 |
| distribution_command.rs | 0 | 0 |
| release_command.rs | 0 | 0 |
| runner/render.rs | 0 | 1 (legitimate — this IS the render layer) |
| runner/error.rs | 1 (From impl) | 1 (From impl) |
| cli_help.rs | 0 | 3 (Theme + Renderer + UiError bridge) |

All subsystem usages in these files are honest. `runner/error.rs` wrapping
each subsystem's error type via `From` is precisely the pattern the lane has
been working toward. `runner/render.rs` using `effigy_ui::Renderer` is
tautological — it's the runner's render boundary. `cli_help.rs` using
`effigy_ui::theme::Theme` for the CLI header is the CLI-contract seam that
`229/230` explicitly paused on.

## Decision

Pause the `g02.010` strict lane entirely on an honest full boundary.

- no bounded runner-shell cleanup target remains outside parallel-thread
  ownership
- all cross-cutting subsystems are now in dedicated crates:
  `effigy-process`, `effigy-ui`, plus earlier `effigy-cli` (with its
  help topic surface via `229`), `effigy-contracts`, `effigy-core`
  (widgets + resolver + fs/path helpers)
- every paused runner file reviewed carries only honest CLI/runtime adapter
  work: command dispatch, final render choice, error-type `From` impls,
  callback wiring, theming
- `g02.007` queued release closure is the natural next active lane; the
  modularization boundary is now trustworthy enough for `v0.3`

## Why This Is The Right Pause

The lane opened because the root crate was carrying too much interleaved
product logic. Across `g02.010`'s execution:

- `effigy-cli`, `effigy-core`, `effigy-tasks`, `effigy-manifest`,
  `effigy-containers`, `effigy-distribution`, `effigy-release`,
  `effigy-bootstrap`, `effigy-changelog`, `effigy-rhai`, `effigy-demo`,
  `effigy-docs-policy`, `effigy-env`, `effigy-doctor`, `effigy-contracts`,
  `effigy-tui`, plus the new `effigy-catalog`, `effigy-exec`,
  `effigy-gateway`, `effigy-process`, and `effigy-ui` all exist and own
  their domains
- the root crate is now a thin shell over those crates: CLI dispatch,
  `run_and_render_command`, final exit-code mapping, error adaptation
- each domain exposes a Rust API that `effigy-rhai` adapts without
  rewriting the domain

Continuing would push genuine runner wiring (error `From` impls, CLI
header theming, dispatch tables) into crates that shouldn't own those
concerns. That breaks the boundary in the wrong direction.

## Vision Target Delta

- primary vision tags: `MAINT`, `CONTRACT`, `ROUTE`, `RELEASE`
- moved: `g02.010` now paused on a trustworthy full boundary with every
  `g02.017` queue job either done or explicitly left to the parallel
  thread that owns the relevant write-set
- remaining open: resume `g02.007` (release closure) via
  [`115-implement-effigy-distribution-release-closure.md`](../../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)
  when `v0.3` closure is intended

## Validation

- `cargo test` — full workspace green (11 suites)
- `cargo run --bin effigy -- qa:docs` — passes
- `git diff --check` — clean

## Next Task

The `g02.010` strict modularization lane is paused. Resume the queued
release card
[`115-implement-effigy-distribution-release-closure.md`](../../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)
when release closure is intended, or open a new product roadmap card
against the now-stable crate boundary (e.g., `g02.013` dev front door,
`g02.008` demo and manifest import rollout).
