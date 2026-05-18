# Graph Watch Closeout

Date: 2026-05-18
Roadmap: `g07.021`
Strict lane: `088`

## What Changed

- closed the graph watch lane
- closed `g07.022`, `g07.023`, and `g07.024`
- closed batch cards `960` through `964`
- closed strict lane `088`

## Final Proof

Validation surface:

- `cargo test -p effigy-codegraph`
- `cargo test --lib parse_graph_watch_accepts_debounce_repo_and_json_flags`
- `cargo test --lib render_graph_help_shows_index_query_and_context_surface`
- `cargo test --test cli_output_tests cli_graph_watch_json_streams_started_and_refresh_events -- --nocapture`

Live timing sample with default debounce:

- started event latency: `4.9ms`
- refresh after file write: `1035.6ms`
- refresh after file delete: `1022.6ms`
- deleted path surfaced in watch refresh: `src/lib.rs`

Final watch event families:

- `started`
- `refresh`
- `dirty`
- `reconcile`
- `fatal`

## Residual Limits

- watch mode remains foreground-only
- overflow proof is synthetic through watcher-unit coverage, not a kernel-level
  stress harness
- JSON watch mode is a streaming event surface and intentionally does not use
  the normal one-shot `effigy.command.v1` envelope

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- what moved in this report: no graph freshness watcher -> bounded foreground
  watch mode with typed JSON events, debounce-backed updates, and explicit
  dirty/reconcile fallback
- what remains open: None
