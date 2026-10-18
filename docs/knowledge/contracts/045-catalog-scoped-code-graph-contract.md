# 045 - Checkout Code Graph Contract

Architecture: [027](../architecture/027-catalog-scoped-code-graph.md).
Guide: [076](../../guides/076-code-graph-and-agent-workflows.md).

## Authority and surface

The graph owns one derived corpus per checkout, under `.effigy/graph/graph.db`,
with one refresh lock. Catalog membership does not partition that corpus.
The six CLI subcommands are `index`, `status`, `search`, `node`, `affected` and
`explore`. The guide owns command examples and migration replacements.

Tom's 2026-10-18 ruling accepts per-file consistency and mixed file generations
while a refresh is reported honestly. It retires the watcher, file-list command,
catalog-scoped graph and separate relationship/context commands.

## Atomic file updates

One SQLite transaction deletes a file's owned graph and search rows, inserts
its replacement records, and saves scan state with those records. Scan state
includes a per-file extractor/profile freshness key. It never commits before
the rows it vouches for. Search text comes from the same source bytes as the
graph. Extraction failures commit diagnostics with the file state and report
`degraded`, never an apparently complete success.

Deletion is per file. Incoming edges and references belong to their source and
survive replacement of a target file. Typed document relations resolve in
transactions grouped by source file. An interrupted relation pass keeps
Markdown paths stale until the pass completes.

Storage schema 3 introduces per-file keys. The schema migration and version
marker commit atomically, so interrupted upgrades can reopen and resume.
Run metadata, extractor inventory and the git stamp are advisory; they cannot
certify unprocessed files. The git stamp is cleared before mutation.

## Reads and freshness

Plain status opens read-only and creates no state. Queries may refresh the
lazy derived cache. A query reads one SQLite transaction over committed rows;
it may observe different generations for different files. A held refresh lock
means `refreshing (n/N)`, with holder identity, even if some files are already
current. A packet before any files have committed is unusable and empty.
An interrupted writer leaves committed files intact; remaining stale files
are reported by status after reopening.

`--stale-index` skips refresh and lock wait, reporting `stale-index` or
`refreshing`. A cold stale read returns `missing-index` with recovery guidance.
A complete-database snapshot is never produced. Refresh removes the obsolete
complete database; orphaned catalog stores are ignored and left untouched,
with one stderr warning per graph invocation.

## Deadline

Query refresh runs synchronously on the caller's thread. The cooperative
deadline covers walk steps, freshness scans, lock polls and file transaction
boundaries. The current transaction finishes or rolls back before work returns;
there is no detached worker or unbounded spawn-failure fallback. Timeout JSON
identifies the last phase, progress and `committed: true`. Retries resume
committed progress. Explicit index remains unbounded.

## Query packets and compatibility

The common envelope has `schema`, `schema_version`, `command`, `repo_root` and
`payload`. It has no catalog or fan-out field.

Node accepts a file path or stable ID, defaults to both directions and supports
`--in` or `--out`. Its payload includes the seed file/symbol, selected `files`,
`symbols`, `edges`, `references`, `diagnostics`, `target`, `direction` and
freshness. The default limit of 100 bounds each collection.

Explore uses `effigy.graph.explore.v1` for both modes. `mode: navigation` keeps
the primary/excerpt/relation packet. `mode: excerpts-only` keeps the ranked
`request`, `items`, `overflow`, `freshness` and `notes` packet. Exact-token
confirmation remains outside the graph's syntactic inference.

The [retirement registry](../retired.toml) owns removed spellings. They produce
replacement errors for one release. Removing catalog graph configuration does
not change ordinary catalog routing or task membership.

## Required proofs

Crash at every file graph/search/state/commit and deletion boundary, then
reopen: rows, FTS and scan state agree, and remaining stale paths are exact.
Repeat with deadline expiry and interrupted schema migration. Prove incoming
cross-file records survive target refresh, no snapshot is produced, all catalog
members share the checkout corpus, direction/limit behavior is bounded, removed
spellings name replacements, and orphan warnings do not mutate old stores.
Subcommand/help and distributed/project-local skill parity remain required.
