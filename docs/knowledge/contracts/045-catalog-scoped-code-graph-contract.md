# 045 - Checkout Code Graph Contract

Architecture: [027](../architecture/027-catalog-scoped-code-graph.md).
Guide: [076](../../guides/076-code-graph-and-agent-workflows.md).

## Authority and surface

The graph owns one derived corpus per checkout, under `.effigy/graph/graph.db`,
with one kernel store gate. Catalog membership does not partition that corpus.
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

Contract 052 R2 applies to every graph read, including doctor, status, queries
and documentation context. Reads open SQLite read-only, create no files, take
no exclusive gate and never refresh. Missing or stale data names
`effigy graph index` as the next action. The legacy status `--refresh` flag
warns and remains report-only for one release.

The database uses rollback journaling (`journal_mode=DELETE`). Queries use
`SQLITE_OPEN_READONLY`, a 100ms busy timeout and memory-only temporary storage.
They never open a database with a WAL sidecar or WAL-mode header: that state
reports stale (refresh interrupted or in progress). The next explicit index
converts existing WAL databases to DELETE mode, retaining their committed data.
A query reads one SQLite transaction over committed rows;
it may observe different generations for different files. A held refresh lock
means `refreshing (n/N)`, with holder identity, even if some files are already
current. A packet before any files have committed is unusable and empty.
An interrupted writer leaves committed files intact; remaining stale files
are reported by status after reopening.

`--stale-index` retains its explicit stale trust label, reporting `stale-index` or
`refreshing`. A cold stale read returns `missing-index` with recovery guidance.
A complete-database snapshot is never produced. Refresh removes the obsolete
complete database; orphaned catalog stores are ignored and left untouched,
with one stderr warning per graph invocation.

## Deadline

Explicit indexing holds a kernel `Gate(Store graph:<cache-location digest>)`
in machine scope; canonical checkout paths share a derived-cache gate across
path aliases. The gate lives in the host kernel state root. Observe probes use
only the kernel's nonblocking shared-lock probe, never the old exclusive
refresh-lock probe. A copied or moved cache gets its own store location; this
derived-cache exclusion is separate from managed-run checkout identity.

Indexing runs synchronously on the caller's thread. The cooperative
deadline covers walk steps, freshness scans, gate waits and file transaction
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

R2 proofs cover missing, stale and WAL reads, and command-level filesystem
sentinels for status, search and doctor during and after an actual per-file
refresh. They also prove committed progress remains queryable during refresh.
