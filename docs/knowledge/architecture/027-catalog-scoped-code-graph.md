# Checkout Code Graph Architecture

Contract: [045](../contracts/045-catalog-scoped-code-graph-contract.md).

The graph has one checkout corpus, one SQLite database, one refresh lock and
one cooperative deadline. Catalog topology does not create another ownership
model. Both code navigation and documentation context use this same corpus.

The unit of index consistency is a file. Parsing happens before mutation;
graph rows, FTS rows and the per-file freshness key commit together. WAL lets
concurrent readers keep a stable SQL view while the writer advances through
files. Mixed committed file generations are reported as `refreshing (n/N)`.
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
