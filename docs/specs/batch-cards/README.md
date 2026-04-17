# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards; archive stale cards
  once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of every historical card

## Current Live Chain

- [`250-implement-effigy-builtin-extraction.md`](./250-implement-effigy-builtin-extraction.md)
  is complete. `src/runner/builtin/**` (120 files, ~10,114 lines)
  moved into the new `effigy-builtin` workspace crate behind a
  narrow `BuiltinError` → `RunnerError` boundary. The
  `BuiltinRuntimePorts` trait definition plus `LockScope`,
  `UnlockResult`, `TaskCacheEntry` moved with it; the runner keeps
  the concrete `RunnerBuiltinPorts` impl. `BUILTIN_TASKS` and
  `DEFAULT_BUILTIN_TEST_MAX_PARALLEL` now live in the new crate as
  `pub const`s. With `g02.010`'s bounded batch exhausted, the
  strict lane moves to a pause-boundary decide card.
- [`251-implement-builtin-runtime-ports-inversion.md`](./251-implement-builtin-runtime-ports-inversion.md)
  is complete. The port-trait inversion landed with 13 methods (not
  16 as proposed — three unused cache-lifecycle methods dropped after
  grep confirmed builtin doesn't consume them). Pure-helper
  relocations shipped: `parse_task_selector` → `effigy-tasks`,
  `with_local_node_bin_path` direct from `effigy_core::shell` at
  every site, `render::{plain_renderer, render_utf8, text_renderer,
  encode_json}` → `effigy-ui`.
- [`115-implement-effigy-distribution-release-closure.md`](./115-implement-effigy-distribution-release-closure.md)
  is complete. Release execution remains deferred until the `g02.010` thread
  closes cleanly.

## Archive Rule

- closed or paused lane cards should move to `../archive/batch-cards/` once the
  lane no longer needs them in the active tree
- the active tree should stay focused on the live strict lanes rather than the
  full historical corpus
- use the governing spec plus roadmap to resolve the current ready card; this
  README is only the front door

## Next Task

Plan the `g02.010` pause-boundary decide card. With the last
bounded batch (built-in tasks) extracted, spec 010's active scope
is exhausted. Options for the next step include:

- resume release closure (card `115`'s deferred execution path),
- open a fresh modularization spec for the remaining coupling (e.g.
  runner-internal `locking` / `cache` / `execute` cleanups), or
- pause strict-lane execution entirely and hand the roadmap back to
  planning for a new pivot.

Draft the decide card once a steering signal arrives; until then
the `g02.010` lane sits closed.
