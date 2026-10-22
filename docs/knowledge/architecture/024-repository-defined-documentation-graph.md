# Repository-Defined Documentation Graph Architecture

[Contract 041](../contracts/041-documentation-graph-profile-contract.md)
owns the profile grammar. v0.15 removed the readers that consumed it:
`effigy graph`, `effigy docs context`, the `effigy-codegraph` crate and the
three graph-backed scanners. The profile stays parseable and validated so
existing manifests keep working; nothing reads it at runtime.

## Purpose

The profile let a repository name its documentation kinds, metadata fields,
currentness values, authority weights and typed link relations, so retrieval
could rank exact sections by repository-owned authority instead of raw text
similarity. That retrieval surface is gone; `docs check` and the
`docs/README.md` index are the documentation surfaces that remain.

## What remains

- `[docs_policy.graph]` is still parsed and validated by `effigy-manifest`
  through `load_docs_policy_graph_config`, and `effigy config --schema`
  still renders its shape.
- The committed profile is copied configuration, not inherited runtime
  behaviour: no starter, installed skill or template cache is consulted.
- `docs check` index, next-action, link, heading, path and JSON-example checks
  are unchanged and do not use the profile.

## What was removed

- One SQLite graph under `.effigy/graph/`, its freshness lifecycle, per-file
  transactions, lexical retrieval, typed-relation expansion and bounded
  `effigy docs context` payloads.
- The `effigy-codegraph` crate, its extractors, its store, and the
  `effigy.graph.*` / `effigy.docs.context*` JSON contracts.
- `[docs_policy.sources]` cross-repository routing: the table is still
  validated, but no command consumes it.

Historical `.effigy/graph/` state stays untouched. A future decision can
retire or replace the profile; until then it is validated config with no
reader, and `docs/README.md` plus `rg` are how agents find documentation
authority.

## Non-Goals

- a Northstar-only schema
- a second database, background daemon, or required MCP server
- model-generated summaries as authority
- silently rewriting an existing committed profile
