# 076 - Code Graph And Agent Workflows

Use the graph for code ownership, call neighborhoods and changed-file impact.
Use `tasks` for selector inventory, `doctor` for routing or health ambiguity,
and `test --plan` when test selection matters. Exact-token confirmation belongs
to `rg`.

## Start with the question

```sh
effigy graph explore "trace release orchestration" --max-files 6 --max-bytes 12288 --json
effigy graph search release --limit 20 --json
effigy graph node src/runner/graph_command.rs --json
```

Queries build or refresh the local index on demand. Explicit `graph index`
pre-warms the cache or resumes a large build before you ask a question.

## Six commands

| Command | Purpose |
| --- | --- |
| `graph index` | Explicitly build or refresh the whole checkout |
| `graph status` | Inspect freshness, counts and stale paths |
| `graph search <QUERY>` | Search indexed names and records |
| `graph node <ID\|PATH>` | Inspect a file or symbol and its relationships |
| `graph affected <PATH>...` | Find likely validation targets after edits |
| `graph explore <REQUEST>` | Return an agent navigation packet |

Plain `graph status` is report-only and never creates an index.
`--refresh` is the explicit mutating exception:

```sh
effigy graph status --json
effigy graph status --refresh --json
```

`node` accepts stable graph IDs or repository-relative file paths. Omit both
direction flags for both incoming and outgoing relationships. A path includes
the file's declarations and their relationships. `--limit` bounds each result
collection, defaulting to 100.

```sh
effigy graph node symbol:rust:run_release --in --limit 20 --json
effigy graph node symbol:rust:run_release --out --limit 20 --json
```

`explore` returns primary owners, excerpts, relations, edit targets and likely
tests. `--excerpts-only` returns the lower-level ranked excerpt packet instead.
Both modes accept file and byte budgets, repeated language filters and repeated
path prefixes. The JSON payload identifies its mode as `navigation` or
`excerpts-only`.

```sh
effigy graph explore "trace release orchestration" --excerpts-only --max-files 4 --max-bytes 8192 --language rust --path src/ --json
git diff --name-only | effigy graph affected --stdin --depth 2 --limit 30 --json
```

Affected results are likely validation targets, not exhaustive reachability or
permission to skip required checks. Graph extraction is syntactic; dynamic
calls, generated files and ignored dependencies may be absent.

## One checkout, one store

The graph uses `.effigy/graph/graph.db` and a colocated refresh lock. Declared
catalog members belong to the same corpus. Narrow an explore result with
`--path`; no catalog partition is refreshed separately. Orphaned catalog stores
are ignored, with one stderr warning per graph invocation when found. They are
left untouched.

Each file update commits graph rows, full-text search rows and its freshness key
in one SQLite transaction. The key includes the extractor version and relevant
documentation-profile fingerprint. Deletes remove the file's own rows and keep
incoming records owned by unchanged files. A query holds a SQLite read
transaction so its component reads agree.

There is no full-database snapshot copy. During another process's refresh,
queries immediately read committed files and report `refreshing (n/N)` with
lock identity. Different files can represent different committed generations.
Before the first file commits, a refreshing packet is unusable and has no
records. After interruption, status reports the remaining stale files.

## Deadlines and stale reads

Data queries have a default 120000ms cooperative deadline. Configure
`EFFIGY_GRAPH_TIMEOUT_MS=<MS>`; `0` disables the bound. Explicit indexing is
unbounded. Refresh checks the deadline at safe boundaries, finishes or rolls
back the current file transaction and stops. No detached worker continues
writing after the command returns. Retry to resume committed progress.

Timeout details use `effigy.graph.timeout.v1`, including the last phase,
`done`, `total` and `committed: true`. Counts can be null before a counted phase.

```sh
effigy graph explore "trace release orchestration" --stale-index --json
```

`--stale-index` skips refresh and lock wait. It returns committed data marked
`stale-index`, or `refreshing` while a writer holds the lock. Neither label
means current. A cold stale read returns `missing-index` with a next action.
Use the freshness state's `usable` flag before consuming results.

## Migration in v0.15

| Removed spelling | Replacement |
| --- | --- |
| `graph watch` | Queries refresh automatically; use `graph index` to pre-warm |
| `graph files` | `graph search <QUERY>` or `graph node <PATH>` |
| `graph callers <ID>` | `graph node <ID> --in` |
| `graph callees <ID>` | `graph node <ID> --out` |
| `graph impact <TARGET>` | `graph node <ID\|PATH>` |
| `graph context <REQUEST>` | `graph explore <REQUEST> --excerpts-only` |
| `[catalog.graph]` | Remove the table; the whole checkout is indexed |
| `--catalog`, `--all-catalogs` | Use the checkout graph; narrow explore with `--path` |

Removed command spellings and catalog graph settings return replacement errors
for one release. `--stale-index` is only a data-query option; status already
reports without refreshing.

For a damaged derived cache, preserve it before rebuilding:

```sh
mv .effigy/graph .effigy/graph.backup-$(date +%s)
effigy graph index --json
```

## Related references

- [JSON contracts](017-json-output-contracts.md)
- [JSON examples](026-json-payload-examples.md)
- [Code graph contract](../knowledge/contracts/045-catalog-scoped-code-graph-contract.md)
- [Documentation context](079-documentation-graph-profiles-and-context.md)
