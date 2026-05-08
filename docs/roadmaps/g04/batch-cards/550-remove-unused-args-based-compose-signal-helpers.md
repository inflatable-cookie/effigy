# 550 - Remove Unused Args Based Compose Signal Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make runtime signal helpers plan-first by deleting unused args-based compose
helpers.

## Scope

- remove unused args-based compose capture/inherited-session helpers from
  `crates/effigy-runtime/src/signals.rs`
- remove the remaining `compose_invocation` import from runtime signals if it
  becomes unused
- keep plan-based capture and inherited-session helpers
- keep container exec compatibility helpers in `effigy-containers` unchanged

## Non-Goals

- no runner behavior changes
- no signal handling redesign
- no public CLI changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runtime signals expose only plan-based compose
side-effect helpers and focused runtime/lib checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-signal-prune-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-signal-prune-libcheck cargo check -p effigy --lib`
- PASS:
  `rg 'run_compose_capture_from_args|spawn_compose_inherit_from_args|run_compose_inherit_with_stop_flag|compose::compose_invocation' crates/effigy-runtime/src/signals.rs`
  returned no matches
- PASS: `git diff --check`

## Next Task

Split runtime data volume/cache planning helpers out of `data.rs`.
