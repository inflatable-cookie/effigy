# Graph Watch Backend And Debounce

Date: 2026-05-18
Roadmap: `g07.022`
Strict lane: `088`

## What Changed

- added `graph watch` to the graph command surface
- added a first-party watcher loop in `effigy-codegraph` using `notify`
- defaulted watch debounce to `1000ms`
- coalesced burst events into one incremental `graph index` refresh
- added newline-delimited JSON watch events and text-mode watch summaries
- special-cased CLI dispatch so the streaming watch surface bypasses the normal
  one-shot command envelope

## Proof

- parser coverage:
  - `parse_graph_watch_accepts_debounce_repo_and_json_flags`
- help coverage:
  - `render_graph_help_shows_index_query_and_context_surface`
- codegraph crate:
  - `cargo test -p effigy-codegraph`
- CLI stream proof:
  - `cli_graph_watch_json_streams_started_and_refresh_events`

Observed watch event shape:

- `started`
- `refresh`
- `watcher-error`
- `fatal`

The JSON watch stream is newline-delimited and uses schema
`effigy.graph.watch.event.v1`.

## Residual Limits

- overflow and dirty-reconcile behavior are not closed yet
- the current watcher surface does not expose detach/service mode
- the watch backend still watches the repo recursively and filters noisy paths
  in-process rather than relying on backend-native exclusion

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- what moved in this report: no graph watch implementation -> first working
  foreground watch surface with typed JSON events and debounce-backed refresh
- what remains open: `963` overflow/reconcile hardening and `964` closeout proof
