# 076 - Code Graph And Agent Workflows

Use this guide when you want the fastest safe path to repo context before broad
file scanning. This is the code-understanding lane inside Effigy's wider agent
surface, not the default front door for every task.

Effigy's graph is a local, deterministic repo index under `.effigy/graph/`.
It is built from first-party extractors and queried through the CLI. It is not
an MCP server, not a daemon, and not compiler-grade semantic truth.

## Start Here

Do not front-load `doctor`, `tasks`, and `test --plan` when the job is already
clearly code understanding. Go straight to graph when the question is
code-navigation shaped: where a behavior lives, how a path flows, what changed
impact looks like, or which files to read first.

Use the graph in this order:

```sh
effigy graph explore "trace release orchestrator" --max-files 6 --max-bytes 12288 --json
effigy graph status --json
```

That gives you:

- a local index — built or refreshed on demand as part of the query
- freshness state
- a bounded starting packet with primary owners, excerpts, related symbols,
  reasons, guidance, and overflow

You normally do not need to run `effigy graph index` first: queries rebuild a
stale or missing index themselves. Pre-warm explicitly only when you want to
pay the indexing cost up front (large repos, batch runs).

Do not force this path onto unrelated jobs. If the work is execution,
deployment, state orchestration, docs validation, release inspection, or repo
health, use the matching Effigy surface first.

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

`graph index` is explicit. Graph data queries (`search`, `node`, `callers`,
`callees`, `impact`, `context`, `explore`, `affected`, `files`) refresh a stale
or missing index on demand, so query results are current without a manual
reindex step. On git repos whose indexed HEAD still matches with a clean
working tree, freshness is verified via `git status` without a full scan;
non-git repos and any git failure (no `.git`, missing `git`, unborn HEAD) fall
back to the per-file scan-state walk. Refreshes run under a cross-process lock
(`.effigy/graph/refresh.lock`); concurrent queries wait a short budget and then
report the true trust state. `graph status` stays report-only — it never
mutates graph state.

Graph data queries have a 120000ms wall-clock budget by default. Set
`EFFIGY_GRAPH_TIMEOUT_MS=<MS>` to override it; `EFFIGY_GRAPH_TIMEOUT_MS=0`
disables the bound. Explicit `graph index` and `graph watch` commands are
unbounded.

### Check freshness

```sh
effigy graph status --json
```

`graph status` is report-only: it never mutates graph state. When you want
the report *and* the remediation in one step, pass `--refresh` to rebuild a
stale or missing index on demand (the same lazy-refresh gate queries use):

```sh
effigy graph status --refresh --json
```

`effigy doctor` also surfaces a stale or degraded graph index as a
`graph.index` warning with the `graph status --refresh` remediation; a
missing index stays silent because queries rebuild it on demand.

Look at:

- `freshness.state` — compact trust label
- `freshness.usable` — whether graph queries are safe to trust
- `freshness.summary` — one-line operator guidance
- `stale_paths`, `new_paths`, `changed_paths`, `deleted_paths`, `failed_paths`

Trust states:

| `freshness.state` | Meaning |
| --- | --- |
| `ready` | Index is current enough for navigation |
| `refresh-recommended` | Stale paths exist; status reports it — queries refresh on demand |
| `degraded` | Partial index problems; treat output as bounded guidance |
| `missing-index` | No files indexed yet; queries build on demand |

Query payloads are refreshed before they are served, so a stale index no
longer poisons `explore`, `affected`, or `context` output. The
correctness-gated scans (`scan dead-code`, `scan validation-gaps`,
`scan boundary-violations`) refresh through the same gate before running, so a
stale or missing index no longer blocks them — they only refuse when the
refresh itself cannot complete. `graph status` and `--graph-context` reporting
use different postures: status stays report-only unless passed `--refresh`,
while graph-context scan enrichment refreshes before reading graph facts.

If the graph DB is corrupt or you hit an unsupported future storage schema,
rebuild it locally:

```sh
mv .effigy/graph .effigy/graph.backup-$(date +%s)
effigy graph index --json
```

The graph is a cache. Move it aside so the old state remains recoverable while
the replacement index is built.

## Lazy Refresh: How Freshness Works

Graph data queries are read-your-writes surfaces: instead of returning stale
results, each query makes the index current first. The lifecycle:

1. **Detect.** The query runs a freshness pass before reading any graph data.
2. **Skip or walk.** On a git repo a fast gate can prove the index is current
   without touching the tree. Otherwise a per-file scan-state walk compares
   stored mtime/size/content hashes against the working tree — the same
   detection `graph status` reports.
3. **Refresh on demand.** A stale or missing index is rebuilt incrementally
   (only changed files are re-extracted) under a cross-process lock.
4. **Serve with honest trust state.** If another process is mid-refresh and
   the wait budget expires, the query serves what exists and marks it
   `refresh-recommended` — it never claims freshness it does not have.

### The git fast path

When an index is built from a clean working tree, Effigy records the git HEAD
in the graph metadata. A query counts as fresh without a scan when:

- the recorded HEAD matches the current `git rev-parse HEAD`, and
- `git status --porcelain` shows no changes outside paths the graph walk skips
  (`.git/`, `.effigy/`, `.venv/`, `__pycache__/`, `coverage/`, `dist/`,
  `node_modules/`, `target/`, `vendor/`, `.svelte-kit/`, `.next/`, `.nuxt/`,
  `.output/`, `.turbo/`, `.parcel-cache/`).

The stamp is written only on a clean-tree index. An index built over
uncommitted edits carries no stamp, so the gate can never mistake a dirty-tree
snapshot for the committed tree: if you edit and then revert, the walk still
finds the drift and refreshes.

Git is an optimization, not a requirement. Every git failure — no `.git`,
missing `git` binary, unborn HEAD, uncommitted changes — falls back to the
scan-state walk, exactly the behavior that existed before the gate. Graph
queries never require git.

### Concurrency

A cross-process lock (`.effigy/graph/refresh.lock`) guarantees only one
process re-indexes at a time. `graph index`, `graph watch` batches, the gated
scans, `graph status --refresh`, and lazy query refreshes all take the same
lock. When a query finds the lock held it waits a short budget (2.5s), then
re-checks: if the other process finished, it serves fresh data; otherwise it
serves what exists with the honest trust state. Two processes never race to
rebuild the same graph.

### Surface summary

| Surface | Behavior |
| --- | --- |
| `graph search/node/callers/callees/impact/context/explore/affected/files` | refresh a stale or missing index before serving |
| `scan dead-code`, `scan validation-gaps`, `scan boundary-violations` | refresh through the same gate; refuse only if the refresh cannot complete |
| `scan ... --graph-context` | refresh before enriching supported scan findings; report refreshed readiness for unsupported families |
| `graph status` | report-only by default; `--refresh` opts into the gate |
| `graph index`, `graph watch` | explicit rebuild paths; share the lock |
| `effigy doctor` (`graph.index` check) | warns on a stale or degraded index with the `graph status --refresh` remediation |

### Cost and the one-time build

The common case is cheap: on a clean git repo a query costs one `git status`
check instead of a tree walk. On a dirty or non-git repo it costs one stat
walk, unchanged from before lazy refresh. Only the first query after edits
pays the incremental reindex, and that is proportional to what actually
changed.

The exception is the first query ever on a large repo: no index exists, so
the query builds the whole thing. That is a one-time cost — warm the index
ahead with `effigy graph index` when onboarding a big repository, or rely on
`effigy graph watch` during long sessions.

### Trust states after a query

`graph status` is the honest pre-refresh view; query payloads are the
post-refresh view. When a query refreshes, the freshness summary records it
(e.g. `graph index is current (graph auto-refreshed (2 files in 41ms))`), so
agents can see what the answer cost.

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
- graph-aware scan proof cases under `scan_cases` when fixture scan checks run

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

## Graph-Native Scan Example

When a repo wants graph-backed architecture checks, use a scan rule instead of
hard-coded repo logic.

Path-layer rules are the first supported shape:

```toml
[scan.boundary_violations]
doctor = false

[scan.boundary_violations.layers.app]
paths = ["src/app/**"]
may_depend_on = ["domain", "shared"]

[scan.boundary_violations.layers.domain]
paths = ["src/domain/**"]
may_depend_on = ["shared"]

[scan.boundary_violations.layers.shared]
paths = ["src/shared/**"]
```

Then run:

```sh
effigy scan boundary-violations --json
```

The scan refreshes a stale or missing graph index through the lazy-refresh
gate before running.

Current behavior:

- rules are optional
- repos with no layers return a clean no-rules result
- resolved edges are checked directly
- heuristic edges stay excluded unless the scan config opts into them

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
- takes the same cross-process refresh lock as lazy query refreshes, so
  parallel refresh paths never race
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

1. `effigy graph explore "<task>" --max-files 6 --max-bytes 12288 --json` — a
   stale or missing index is refreshed on demand as part of the query
2. trust returned excerpts for first-pass orientation
3. only then widen to `graph context`, `graph search`, `graph node`,
   `graph callers/callees`, direct file reads, and `rg`
4. use `effigy graph status --json` when you need the honest pre-refresh trust
   state or index counts

Do not front-load `doctor`, `tasks`, or `test --plan` when the job is already
plain code understanding. Use those surfaces when the job is routing
ambiguity, selector inventory, or test-shape discovery.

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

## Graph-Native Scans

Use graph-native scans when the relationship data is the point.

All three graph-native scans (`dead-code`, `validation-gaps`,
`boundary-violations`) refresh a stale or missing index through the
lazy-refresh gate before running, so a stale index no longer blocks them; they
only refuse when the refresh itself cannot complete.

Dead-code review is advisory:

```toml
[scan.dead_code]
doctor = false
allow_paths = ["src/bin/**", "scripts/**"]
allow_symbols = ["crate::bootstrap::*", "main"]
```

Run it like this:

```sh
effigy scan dead-code
effigy scan dead-code --json
```

Current dead-code finding types:

- `isolated-file`
- `unreferenced-symbol`

Safe review posture:

- treat findings as candidates, not proof
- allowlist intentional bootstrap or entrypoint code before retrying
- confirm behavior in source before deleting code
- do not use the scan as a substitute for compiler dead-code analysis

Validation-gap review is also advisory:

```toml
[scan.validation_gaps]
doctor = false
hotspot_threshold = 4
affected_depth = 2
allow_paths = ["src/bin/**", "scripts/**"]
```

Run it like this:

```sh
effigy scan validation-gaps
effigy scan validation-gaps --path src/lib.rs
git diff --name-only | effigy scan validation-gaps --stdin --json
```

Current validation-gap finding types:

- `hotspot-without-nearby-tests`
- `changed-owner-without-test-target`

Safe review posture:

- treat likely tests as bounded graph hints, not coverage proof
- keep `include_heuristic = false` for resolved graph evidence only; set it to
  `true` when unresolved symbol-name matches should count as nearby tests
- use changed-path mode when the review question is "what should I validate now?"
- use hotspot mode when the review question is "which central owners lack nearby tests?"
- keep release or merge gates on explicit task/test surfaces, not on this scan alone

## Limits

Be explicit about the limits:

- lexical search can still be faster with raw `rg`
- graph edges are not compiler-grade
- ignored/generated/vendor paths are excluded by default
- graph queries refresh a stale index before serving, so results track the
  working tree at query time; the cost of that guarantee is the refresh itself
- the graph walk prunes the current skip list above at any depth

## Related References

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`../architecture/024-repository-defined-documentation-graph.md`](../architecture/024-repository-defined-documentation-graph.md)
- [`../contracts/041-documentation-graph-profile-contract.md`](../contracts/041-documentation-graph-profile-contract.md)
