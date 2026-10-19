# Graph Assist

Use graph for code ownership, flow, implementation and changed-file impact.
Use `tasks` for selector inventory, `doctor` for routing or health, and
`test --plan` when test shape matters. Exact-token and missing-symbol proof
belongs to `rg`.

| Question | Command |
| --- | --- |
| Where is this behavior implemented? | `graph explore <REQUEST>` |
| Which tests likely cover these edits? | `graph affected <PATH>...` |
| Is the index stale? | `graph status` |
| Give me only ranked excerpts | `graph explore <REQUEST> --excerpts-only` |
| Known term, unknown symbol ID | `graph search <QUERY>` |
| File or symbol neighborhood | `graph node <ID\|PATH> [--in\|--out]` |

```sh
effigy graph explore "trace release orchestration" --max-files 6 --max-bytes 12288 --json
git diff --name-only | effigy graph affected --stdin --json
```

Queries and status read committed data without writes or refreshes. Run
`graph index` explicitly when the packet reports missing or stale data. The
legacy status `--refresh` flag warns and stays report-only. Node defaults to both directions and its limit bounds
each collection. Explore accepts repeated language and path filters.

The graph uses one `.effigy/graph/graph.db`, including catalog members. Each
file commits graph, search data and freshness together. Concurrent readers see
committed files with `refreshing (n/N)`. No watcher or snapshot copy is needed.
Queries have a cooperative 120000ms deadline; `EFFIGY_GRAPH_TIMEOUT_MS=<MS>`
overrides it and `0` disables it. Explicit indexing is unbounded. Deadline expiry
stops at a safe boundary and preserves committed progress for the next refresh.

All reads skip refresh and exclusive gate waits. `--stale-index` keeps an explicit stale label. It reports `stale-index`, or
`refreshing` during a writer; neither means current. A cold stale read reports
`missing-index`. Gate on freshness and `usable` before trusting a packet.

For risk questions, use `scan boundary-violations`, `scan dead-code` or
`scan validation-gaps --stdin --json` with changed paths. These narrow likely
risk; graph reachability is syntactic and does not replace required checks.

| Command | Payload schema |
| --- | --- |
| `graph index` | `effigy.graph.index.v1` |
| `graph status` | `effigy.graph.status.v1` |
| `graph search` | `effigy.graph.search.v1` |
| `graph node` | `effigy.graph.node.v1` |
| `graph affected` | `effigy.graph.affected.v1` |
| `graph explore` (both modes) | `effigy.graph.explore.v1` |

Explore's payload mode is `navigation` or `excerpts-only`. See
`docs/guides/076-code-graph-and-agent-workflows.md` for the full contract and
migration guidance.
