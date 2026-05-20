# 076 - Code Graph And Agent Workflows

Use this guide when you want the fastest safe path to repo context before broad
file scanning. This is the code-understanding lane inside Effigy's wider agent
surface, not the default front door for every task.

Effigy's graph is a local, deterministic repo index under `.effigy/graph/`.
It is built from first-party extractors and queried through the CLI. It is not
an MCP server, not a daemon, and not compiler-grade semantic truth.

## Start Here

Start with the normal repo loop first:

```sh
effigy doctor
effigy tasks
effigy test --plan
```

Then switch to graph when the question is code-navigation shaped: where a
behavior lives, how a path flows, what changed impact looks like, or which
files to read first.

Use the graph in this order:

```sh
effigy graph index
effigy graph status --json
effigy graph explore "trace release orchestrator" --max-files 6 --max-bytes 12288 --json
```

That gives you:

- a local index
- freshness state
- a bounded starting packet with primary owners, excerpts, related symbols,
  reasons, guidance, and overflow

Do not force this path onto unrelated jobs. If you already know the task is
execution, deployment, state orchestration, docs validation, or release
inspection, use the matching Effigy surface first.

If the repo is changing while you work:

```sh
effigy graph watch --json
```

## What The Graph Is For

Use the graph when you need:

- the first files to read for a task
- where a symbol or behavior is owned
- call or reference neighborhoods
- bounded machine-readable context for an agent

Use other Effigy surfaces first when you need:

- repo health, task discovery, or test routing
- direct task execution
- deploy, distribution, state, docs, contracts, or container workflows

Do not use the graph as the only truth source when you need:

- exact type inference
- compiler-grade call resolution
- dynamic runtime behavior
- generated or ignored paths that Effigy intentionally skips

## Core Commands

### Build or refresh the index

```sh
effigy graph index
effigy graph index --json
```

This writes local graph state under:

- `.effigy/graph/graph.db`

`graph index` is explicit. Queries do not rebuild the graph for you.

### Check freshness

```sh
effigy graph status --json
```

Look at:

- `ready`
- `stale_paths`
- `new_paths`
- `changed_paths`
- `deleted_paths`
- `failed_paths`

If `stale_paths` is not empty, re-run `graph index` before trusting query
results.

If the graph DB is corrupt or you hit an unsupported future storage schema,
rebuild it locally:

```sh
rm -rf .effigy/graph
effigy graph index --json
```

The graph is a cache. Rebuild is the supported recovery path.

### Search

```sh
effigy graph search release --limit 10 --json
```

Use this when you know a term, not yet a symbol id.

### Inspect one node

```sh
effigy graph node symbol:rust:crate::runner::run_release --json
```

Use this after search, callers, callees, or context gives you a stable id.

### Call neighborhoods

```sh
effigy graph callers <ID> --limit 20 --json
effigy graph callees <ID> --limit 20 --json
```

Use these when you already have a symbol id and need impact around it.

### Impact

```sh
effigy graph impact src/runner/release_command.rs --limit 20 --json
effigy graph impact symbol:rust:crate::runner::run_release --limit 20 --json
```

Use this when the starting point is a file path or a known symbol.

### Changed-file validation narrowing

```sh
effigy graph affected src/runner/graph_command.rs --json
git diff --name-only | effigy graph affected --stdin --depth 2 --json
```

Use this when the question is "what should I validate after these edits?".

`graph affected --json` returns:

- `changed_paths`
- `freshness`
- `depth`
- `affected_files`
- `likely_test_files`
- `likely_test_tasks`
- `notes`

Interpret confidence like this:

- `exact`: selected through resolved file or symbol graph facts
- `heuristic`: selected through unresolved target matching or looser evidence

This is a narrowing tool, not an exhaustiveness proof. It should help an agent
choose a smaller validation target before widening to full-suite checks.

### Bounded agent context

Use `graph explore` first when an agent needs to understand a task-shaped
question with fewer immediate file reads:

```sh
effigy graph explore "trace deploy provider export" \
  --max-files 6 \
  --max-bytes 12288 \
  --json
```

`graph explore --json` returns:

- `query`
- `index` freshness and counts
- `summary`
- `primary` owner files/docs
- `excerpts` with paths, ranges, reasons, and text
- `excerpts[*].section_kind` and `excerpts[*].completeness`
- `relations` such as related symbols, files, or docs with explicit traversal
  reasons when `explore` follows a bounded one-hop graph edge
- `edit_targets` for the top implementation owner and adjacent wiring/config
  targets when graph evidence is strong enough
- `likely_test_files`
- `likely_test_tasks`
- `overflow`
- `guidance`

Use the returned excerpts for first-pass orientation. Open returned files only
when the excerpt is too small for the edit or review. Use `rg` for exact token
verification, missing symbols, or confirming behavior before editing.

### Cross-repo benchmark

Use the benchmark task when you want a repeatable adoption check instead of an
anecdotal one:

```sh
effigy perf:graph-agent-benchmark
```

The task always runs fixture-backed cases in this repo. When local live targets
exist, it also benchmarks:

- Effigy
- `~/Dev/projects/underlay-reference`
- `~/Dev/legacy/sites/brains`

It writes:

- `.effigy/perf/graph-agent-benchmark/README.md`
- `.effigy/perf/graph-agent-benchmark/summary.json`

The JSON summary is the machine-readable proof surface. It records:

- graph command count
- fallback search count
- graph and `rg` timings
- first-hit correctness
- whether the graph packet was sufficient without broad fallback
- resolved path and suggested test surfaces

Optional live repos skip cleanly when absent. The benchmark is meant to answer
"did graph reduce navigation work here?" rather than to make broad percentage
claims.

Interpret the extra edit/test fields like this:

- `edit_targets`
  - `implementation`: best ranked owner to edit first
  - `wiring`: adjacent implementation/config target surfaced from bounded graph
    adjacency
  - `config`: direct config owner when the query lands on config behavior
- `likely_test_files` and `likely_test_tasks`
  - bounded validation candidates only
  - not an exhaustiveness claim
  - `exact` means resolved graph evidence
  - `heuristic` means looser unresolved-target matching
- `edit_targets[*].confidence`
  - `ranked` means selected from explore owner ranking rather than a resolved
    graph edge

Interpret excerpt completeness like this:

- `complete-section`: the packet contains a full supported local section
- `truncated-section`: the packet found a section boundary but had to cut it
  to fit budget
- `surrounding-context`: useful nearby text, but not a complete trusted section

Today full section extraction is strongest for:

- Python function/class blocks, including decorator-backed route handlers
- Markdown heading sections

Other languages still fall back to bounded surrounding context.

Traversal is bounded. `graph explore` does not do an unbounded walk or claim a
relation it cannot support from the indexed graph. Today that means:

- resolved file/symbol links can surface directly
- unresolved Rust and JS call/import targets can add bounded heuristic
  neighbors when the symbol/path match is strong enough
- supported manifest and Python entrypoint facts can surface exact
  `entrypoint-task` and `route-handler` relations
- exact token lookup still belongs to `rg`

For implementation-shaped questions, ranking prefers implementation files with
source-body evidence and distinct request-term coverage. Docs and comments can
still appear when relevant, but they should not outrank owner code for queries
such as "where are task routes parsed" or "find graph status stale detection".

Use `graph context` when you want the lower-level ranked item packet:

```sh
effigy graph context "trace deploy provider export" \
  --max-files 8 \
  --max-bytes 4096 \
  --json
```

Filters:

```sh
effigy graph context "trace release orchestrator" \
  --language rust \
  --language markdown \
  --path src/runner \
  --path docs/ \
  --max-files 6 \
  --max-bytes 2048 \
  --json
```

Use `graph context` first when an agent needs a bounded reading packet instead
of broad `rg` across the repo and does not need the assembled `explore` shape.

## Watch Mode

If the repo is moving while an agent or human is working, keep the graph warm:

```sh
effigy graph watch
effigy graph watch --debounce-ms 1000
effigy graph watch --json
```

Current posture:

- foreground only
- default debounce `1000ms`
- uses the same incremental `graph index` path as manual refresh
- emits explicit `dirty` and `reconcile` events when the watcher backend is not
  trustworthy
- local SQLite opens with a WAL-backed posture and a short busy timeout so
  reads and refresh writes overlap more cleanly on one workstation

### JSON Watch Contract

`graph watch --json` is a streaming exception.

It emits newline-delimited JSON events with schema:

- `effigy.graph.watch.event.v1`

It does **not** use the one-shot top-level `effigy.command.v1` envelope because
the command is long-running and emits multiple events.

Event kinds:

- `started`
- `refresh`
- `dirty`
- `reconcile`
- `fatal`

## Agent Workflow

Recommended code-understanding sequence:

1. `effigy doctor`
2. `effigy tasks`
3. `effigy test --plan`
4. `effigy graph status --json`
5. if stale: `effigy graph index --json`
6. `effigy graph explore "<task>" --max-files 6 --max-bytes 12288 --json`
7. trust returned excerpts for first-pass orientation
8. only then widen to `graph context`, `graph search`, `graph node`,
   `graph callers/callees`, direct file reads, and `rg`

Good examples:

```sh
effigy graph explore "trace release execute path" --json
effigy graph explore "find deploy provider export owner" --language rust --json
effigy graph context "docs for graph agent workflow" --json
effigy graph search railway --json
effigy graph impact src/runner/graph_command.rs --json
git diff --name-only | effigy graph affected --stdin --json
```

The graph should reduce aimless scanning, not replace source reading.

## Languages And Coverage

Current first-party coverage includes:

- Rust
- Effigy manifests and TOML
- Markdown docs
- PHP
- Python
- JavaScript / TypeScript

The graph stores provenance and ranges for emitted facts, but some edges remain
heuristic by design.

Supported route and entrypoint facts currently include:

- Effigy bootstrap start selectors linked to in-manifest tasks
- Python FastAPI and Flask-style decorator routes:
  - `@app.get("/path")`
  - `@router.post("/path")`
  - `@app.route("/path", methods=[...])`

Not yet covered:

- Django URL modules
- Express/Fastify route surfaces
- Laravel route files
- Rust web framework routers

## Limits

Be explicit about the limits:

- lexical search can still be faster with raw `rg`
- graph edges are not compiler-grade
- ignored/generated/vendor paths are excluded by default
- graph results are only as fresh as the last successful index or watch refresh

## Related References

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
