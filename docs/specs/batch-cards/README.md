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
  is the ready card for `g02.010`. Scope: single `effigy-builtin`
  crate (~10,114 lines, 120 files) with a `BuiltinError` →
  `RunnerError` boundary. With card `251` landed, residual reach-ins
  are trimmed to manifest re-exports, constants, `test_support`
  re-export, and moving the `BuiltinRuntimePorts` trait definition
  into the new crate.
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

Execute card `250` — migrate `src/runner/builtin/**` (all eleven
tasks plus dispatcher / registry / arg_parser / test_support) into
a new `effigy-builtin` crate with a `BuiltinError` → `RunnerError`
boundary. The `BuiltinRuntimePorts` trait migrates into the new
crate as a `pub trait`; the runner provides the concrete impl and
every call site already routes through the single allowed surface.
