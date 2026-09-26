# Repository-defined documentation graph

[Contract 041](../contracts/041-documentation-graph-profile-contract.md) owns the grammar and exact output. Effigy indexes Markdown documents and exact sections alongside code. `effigy docs context "<question>"` retrieves bounded source evidence with path, span, kind, currentness, authority, and relation provenance. It does not synthesize an answer.

A repository owns its `[docs_policy.graph]` profile in the committed `effigy.toml` or included file. The profile names roots, document kinds, metadata fields, currentness values, authority weights, and typed relations. It participates in graph freshness. No installed skill or starter is read at query time. Without a profile, documents and sections remain queryable with generic kind and unknown currentness.

Effigy's profile in [`docs/effigy.docs.toml`](../effigy.docs.toml) ranks knowledge above plan, guides, and triage. That is a repository choice, not a built-in Northstar ontology. `effigy init northstar` emits a lean example for new consumers; they own the copied bytes thereafter.

The graph shares the code graph's database, lazy refresh, exact source spans, and lexical query machinery. A selected repository scope does not silently fan out to siblings. The [user guide](../../guides/079-documentation-graph-profiles-and-context.md) shows command usage.
