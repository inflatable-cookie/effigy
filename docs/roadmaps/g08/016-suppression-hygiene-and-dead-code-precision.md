# g08.016 - Suppression Hygiene And Dead-Code Precision

Status: Complete
Depends on: `g08.015`
Completed: 2026-06-10

## Goal

Close the maintainability findings from the 2026-06-10 post-hardening scan sweep:
a split-brain clippy-suppression strategy, a small set of residual suppressions
and one genuinely dead function, and a precision gap in Effigy's own dead-code
scanner. The outcome is a single source of truth for workspace lints (so a plain
`cargo clippy` matches CI) and a dead-code scan that stops flagging referenced
and test symbols.

## Findings From The Sweep

- **Split-brain clippy allows.** The three workspace-wide clippy allows
  (`too_many_arguments`, `result_large_err`, `type_complexity`) live only as CLI
  `-A` flags (in `config/tasks.toml`, `.github/workflows/ci.yml`, `CLAUDE.md`,
  `AGENTS.md`). There is no `[workspace.lints]`. On top of that, 33 per-site
  `#[allow(clippy::too_many_arguments)]` attributes duplicate the CLI flag. A
  plain `cargo clippy` (no flags) does not match CI, and the
  `scan stale-suppressions` check flags all 33 as high-severity.
- **Residual suppressions.** 9 `#[allow(dead_code)]`, 1
  `#[allow(unreachable_code)]`, and 1 `#[allow(clippy::items_after_test_module)]`
  — each needs a justify-or-remove decision.
- **One genuine dead function.** `compose_logs_tail_args`
  ([`crates/effigy-containers/src/colima.rs`](../../../crates/effigy-containers/src/colima.rs))
  has zero references.
- **Dead-code scanner precision gap.** `scan dead-code` reports 20 findings, but
  19 are false positives: 17 `#[test]` functions and at least one symbol
  (`ensure_workspace_provisioning_ready`) that **is** imported via `use` at
  `src/runner/system_command/workspace/mod.rs:210`. The graph-backed scan is
  missing `use`-import edges and re-flagging test entrypoints.

## Scope

- consolidate the workspace clippy allows into `[workspace.lints.clippy]`
- opt every workspace package into workspace lints (`[lints] workspace = true`)
- remove the 33 redundant per-site `too_many_arguments` allows
- retire the CLI `-A` flags now that the lints are declared in `Cargo.toml`
- justify-or-remove the residual `dead_code` / `unreachable_code` /
  `items_after_test_module` allows
- delete the one confirmed dead function
- fix the dead-code scanner's `use`-import edge resolution and test-entrypoint
  handling so referenced and test symbols are not flagged

## Guardrails

- no behavior change to shipped code; this is lint/suppression posture and one
  dead-symbol deletion
- `cargo clippy` with no extra flags must end green after consolidation, and
  must match what CI enforces
- do not silence a real lint to make the consolidation pass; if a site genuinely
  triggers a lint that is not workspace-allowed, fix the code or scope a
  deliberate per-site allow with a reason
- `.github/workflows/` edits require explicit human approval (Batch C step)
- do not weaken the dead-code scanner to hide the false positives; fix the edge
  resolution so true positives still surface

## Execution Plan

- [x] **Batch A — Workspace lint consolidation (no workflow edit).** Added
  `[workspace.lints.clippy]` (allow `too_many_arguments`, `result_large_err`,
  `type_complexity`) to the root `Cargo.toml`; opted all 35 packages in via
  `[lints] workspace = true`; removed all 33 per-site
  `#[allow(clippy::too_many_arguments)]`. Dropped the redundant `-A` flags from
  `config/tasks.toml`, `CLAUDE.md`/`AGENTS.md` (one symlinked file). `cargo
  clippy --all-targets` with no extra flags is green and matches CI.
- [x] **Batch B — Residual suppression + dead-symbol cleanup.** Deleted
  `compose_logs_tail_args`. Reviewed the 11 remaining allows: all legitimate
  (held-not-read fields, RAII guard, test helpers, generated code,
  items-after-test-module). Two already carried explanatory comments; added a
  reason to the `state` `targets` field. `scan stale-suppressions` floor is now
  11 (down from 44), all deliberate.
- [x] **Batch C — Dead-code scanner precision + CI flag retirement.**
  Root cause: the 20 dead-code "findings" were **stale-index artifacts**, not a
  reference-resolution logic bug. A fresh `effigy graph index` drops them to 0 —
  the `use`-import edge and `#[test]` handling resolve correctly against a
  current index; the stale index reported drifted line numbers and missing
  edges. The scan already refused an *unusable* index but not a *stale* one, so
  a stale-but-usable index produced false positives. Tightened the guard:
  `scan dead-code` now refuses on `freshness.stale` too and directs the operator
  to run `effigy graph index`. Added a regression test
  (`run_manifest_task_builtin_scan_dead_code_refuses_stale_index`). With
  approval (granted 2026-06-10), removed the `-A` clippy flags from
  `.github/workflows/ci.yml` so CI relies on `[workspace.lints]`.

## Governing Contracts

- [`001-working-rules.md`](../../contracts/001-working-rules.md)
- [`030-low-risk-deduplication-contract.md`](../../contracts/030-low-risk-deduplication-contract.md)
  (no-behavior-change cleanup posture)

## Acceptance Criteria

- [x] `[workspace.lints.clippy]` is the single source for the three repo-wide
  allows; all packages inherit via `[lints] workspace = true`
- [x] zero per-site `#[allow(clippy::too_many_arguments)]` remain
- [x] `cargo clippy --all-targets` with no extra flags is green and matches CI
- [x] every remaining `#[allow(...)]` is either removed or carries a reason;
  `scan stale-suppressions` at its justified floor (44 → 11)
- [x] `compose_logs_tail_args` is deleted; the build stays green
- [x] `scan dead-code` no longer presents stale-index false positives: it
  refuses a stale index with remediation (proven by a regression test). Against
  a fresh index the repo reports 0 dead-code findings.
- [x] changelog records the lint-config change under `[Unreleased] > Changed`
  and the dead-code stale-index guard under `[Unreleased] > Fixed`

## Next Task

Milestone complete — the post-hardening sweep findings are closed: clippy
suppressions consolidated, the one dead function removed, and the dead-code
scanner hardened against stale-index false positives. `g08` stays open for
whatever scope comes next.
