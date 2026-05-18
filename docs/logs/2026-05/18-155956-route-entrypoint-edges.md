# Route And Entrypoint Edges

Date: 2026-05-18  
Roadmap: [`g07.040`](../../roadmaps/g07/040-framework-route-and-entrypoint-edges.md)  
Batch card: [`989`](../../roadmaps/g07/batch-cards/989-add-framework-route-entrypoint-edges.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- added exact manifest entrypoint facts for bootstrap start selectors
  - selector symbols with `kind = "task-selector"`
  - exact `entrypoint-task` edges when the selector resolves to an in-manifest
    task
  - unresolved `entrypoint-task` edges when it does not
- added first Python web route facts
  - `http-route` symbols for FastAPI and Flask-style decorator routes
  - exact `route-handler` edges from route symbols to handler functions
- taught request tokenization to preserve literal route-path tokens such as
  `/users` so route-shaped questions can match route symbols directly
- taught traversal scoring/reasons about `route-handler` and `entrypoint-task`
  edges

## Supported Shapes

Shipped in this slice:

- Effigy `[bootstrap].start` selectors
- Python decorators:
  - `@app.get("/path")`
  - `@router.post("/path")`
  - `@app.route("/path", methods=[...])`

Explicitly not claimed yet:

- Django URL modules
- Express/Fastify route tables
- Laravel route files
- Rust router macros/builders

## Validation

- `cargo test -p effigy-codegraph`
- `cargo clippy -p effigy-codegraph -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo fmt --all -- --check`

New regressions:

- `graph_manifest_indexer_emits_bootstrap_task_selector_entrypoints`
- `graph_python_indexer_emits_route_handler_edges_and_route_queries_find_owner`

## Interpretation

- the graph now carries the first true external-entrypoint facts instead of
  only internal ownership and call/import topology
- route-shaped questions such as `where is /users handled` now have a direct
  symbol path instead of relying on accidental filename or comment matches
- the pattern is now proven for both task entrypoints and one web framework
  family without changing the public JSON contracts

## Residual Limits

- route coverage is still narrow and Python-first
- route traversal is most useful when the route and handler live in the same
  file or when later cards improve section packets and traversal expansion
- `explore` still needs packet hardening to reduce duplicate same-path excerpts
  and make no-reread use more realistic on broad architecture questions

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: the graph now stores first-party entrypoint facts for Effigy bootstrap
  selectors and Python HTTP routes, including exact handler/task edges where
  resolution is reliable
- remains open: stronger source packets, affected-test workflow, wider
  framework coverage, and final parity proof

## Next Task

Execute `990`.
