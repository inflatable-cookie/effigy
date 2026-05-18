# 076 - Code Graph And Agent Workflows

Use this guide when you want the fastest safe path to repo context before broad
file scanning.

Effigy's graph is a local, deterministic repo index under `.effigy/graph/`.
It is built from first-party extractors and queried through the CLI. It is not
an MCP server, not a daemon, and not compiler-grade semantic truth.

## Start Here

Use the graph in this order:

```sh
effigy graph index
effigy graph status --json
effigy graph context "trace release orchestrator" --max-files 8 --max-bytes 4096 --json
```

That gives you:

- a local index
- freshness state
- a bounded starting packet with ranked items, reasons, snippets, and overflow

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

### Bounded agent context

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
of broad `rg` across the repo.

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

Recommended agent sequence:

1. `effigy graph status --json`
2. if stale: `effigy graph index --json`
3. `effigy graph context "<task>" --max-files 8 --max-bytes 4096 --json`
4. only then widen to `graph search`, `graph node`, `graph callers/callees`,
   and direct file reads

Good examples:

```sh
effigy graph context "trace release execute path" --json
effigy graph context "find deploy provider export owner" --language rust --json
effigy graph search railway --json
effigy graph impact src/runner/graph_command.rs --json
```

The graph should reduce aimless scanning, not replace source reading.

## Languages And Coverage

Current first-party coverage includes:

- Rust
- Effigy manifests and TOML
- Markdown docs
- PHP
- JavaScript / TypeScript

The graph stores provenance and ranges for emitted facts, but some edges remain
heuristic by design.

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
