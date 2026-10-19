# Checkout Code Graph Architecture

Contract: [045](../contracts/045-catalog-scoped-code-graph-contract.md).

The graph has one checkout corpus, one SQLite database, one kernel store gate and
one cooperative deadline. Catalog topology does not create another ownership
model. Both code navigation and documentation context use this same corpus.

The unit of index consistency is a file. Parsing happens before mutation;
graph rows, FTS rows and the per-file freshness key commit together. Rollback
journaling lets read-only connections take shared SQLite locks without creating
WAL sidecars. Indexing owns the kernel gate; readers only probe it. A reader
keeps a stable SQL view while the writer advances between file transactions. Mixed committed file generations are reported as `refreshing (n/N)`.
A killed writer cannot leave scan state certifying uncommitted rows.

Cross-file records belong to their source. Replacing a target cannot delete an
unchanged caller's edge or reference. Typed document links are derived from
source declarations; interrupted resolution keeps document paths stale.

Deadlines stop work at safe boundaries on the caller's thread. Committed work
survives and retries resume it. Full database copies and detached timeout
threads have no role in the consistency model.

The six CLI commands project this model without separate watcher, catalog,
file-list or relationship entry points. The guide owns their replacements:
[Code graph workflows](../../guides/076-code-graph-and-agent-workflows.md).

Queries, status, doctor and documentation context never refresh. An existing WAL
store reports stale until explicit indexing converts it to rollback journaling.
This implements contract 052 R2; a read cannot migrate storage or create state.
