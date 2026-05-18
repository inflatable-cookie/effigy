# Graph Watch Dirty Reconcile

Date: 2026-05-18
Roadmap: `g07.023`
Strict lane: `088`

## What Changed

- added explicit `dirty` watch events when the backend produces an untrusted
  condition
- added explicit `reconcile` watch events when a dirty batch is repaired
  through the normal incremental index path
- changed backend-error handling from passive logging to dirty-state fallback
- kept watch refresh on the same `graph index` contract instead of a watcher-only
  mutation path

## Proof

- codegraph crate:
  - `cargo test -p effigy-codegraph`
  - includes:
    - `watch::tests::collect_watch_event_marks_dirty_on_backend_error`
    - `watch::tests::flush_watch_batch_reconciles_deleted_files_when_dirty`
- CLI stream proof:
  - `cargo test --test cli_output_tests cli_graph_watch_json_streams_started_and_refresh_events -- --nocapture`

The fallback proof now covers:

- backend error -> `dirty` event
- dirty batch -> `reconcile` event
- dirty reconcile -> deleted file removed from the graph index

## Residual Limits

- live overflow reproduction is still synthetic through watcher-unit proof, not
  a kernel-level overflow harness
- watch mode still runs only in the foreground
- final lane closeout still needs the consolidated timing and residual-risk log

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- what moved in this report: watch backend errors were only surfaced as loose
  notices -> watch errors now force explicit dirty state and reconcile through
  the canonical incremental indexer
- what remains open: `964` closeout proof
