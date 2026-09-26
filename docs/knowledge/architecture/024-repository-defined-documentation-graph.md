# Repository-Defined Documentation Graph Architecture

[Contract 041](../contracts/041-documentation-graph-profile-contract.md)
owns the grammar and exact output. This document explains why the graph has
its current boundaries and how its components cooperate.

## Purpose

Effigy's code graph already indexes Markdown documents, headings, links, code
fences, path references, and full-text source. That is enough for broad search,
but not enough to answer documentation questions with reliable authority or
currentness. A model can find the right words and still receive an archived guide or incidental triage lead before the live contract.

The documentation graph adds a repository-defined semantic layer to the
existing graph. Effigy owns generic mechanics. Each repository owns the names
and paths that make its documentation authoritative. Northstar is one supplied
profile, not a runtime dependency or built-in ontology.

## Decision

- Keep one graph database and one freshness lifecycle under `.effigy/graph/`.
- Keep the baseline Markdown graph useful when no profile is configured.
- Read the optional profile from the selected repository's `effigy.toml` under
  `[docs_policy.graph]`.
- Let repositories name document kinds, metadata fields, currentness values,
  authority weights, and typed link relations.
- Extract exact sections and deterministic facts. Do not store model-generated
  summaries or inferred policy as canonical graph data.
- Add a bounded `effigy docs context` retrieval surface. It returns evidence
  with provenance; it does not answer the project question itself.
- Ship an example profile through adoption assets. The profile must be copied
  into the consumer repository so behavior does not depend on an installed
  agent skill.

## Repository-Owned Profile

The profile lives in the selected repository's committed `effigy.toml` or an
included manifest file. Names below `fields`, `kinds`, and `relations` are
repository-defined tokens; Effigy reserves no Northstar ontology. This is a
small illustrative shape:

```toml
[docs_policy.graph]
roots = ["README.md", "docs"]

[docs_policy.graph.fields.status]
labels = ["Status"]
cardinality = "many"

[docs_policy.graph.currentness]
field = "status"
current = ["active", "draft"]
historical = ["archived", "superseded"]

[docs_policy.graph.kinds.contract]
include = ["docs/knowledge/contracts/*.md"]
authority = 100
default-currentness = "current"

[docs_policy.graph.relations.contract]
labels = ["Contract", "Contracts"]
```

Kind globs cannot overlap. Unknown keys and repository escapes fail before
query work. Field cardinality, exact spans, currentness ordering, relation
matching, and command budgets are fixed by [contract 041](../contracts/041-documentation-graph-profile-contract.md).

Effigy's own [profile](../../effigy.docs.toml) ranks contracts, architecture,
knowledge, plan, guides, and triage separately. `docs/plan.md` is the plan
surface. The profile is one repository choice, not a built-in Northstar rule.
`effigy init northstar` supplies a lean example; the consumer owns its copied
bytes thereafter.

## Semantic Model

Every in-scope Markdown file remains a document node. A configured profile may
add:

- one repository-defined kind and its authority weight;
- exact section nodes with heading hierarchy and source spans;
- normalized field facts captured from `Label: value` lines;
- current, historical, or unknown currentness;
- typed edges for links found beneath configured headings or on configured
  labelled metadata lines;
- provenance back to the profile entry and exact source location.

Kind match overlap is invalid. Missing metadata is not guessed. An unclassified
document remains queryable with kind `document`, authority `0`, and currentness
`unknown`.

Currentness resolves in this order:

1. a configured field value in the `current` or `historical` set;
2. the matched kind's `default-currentness`;
3. `unknown`.

Profile changes invalidate the documentation semantic layer even when Markdown
bytes are unchanged.

## Retrieval Pipeline

`effigy docs context <QUERY>` uses a deterministic bounded pipeline:

1. ensure the shared graph is fresh;
2. find lexical seeds only inside configured roots, or all Markdown when no
   profile exists;
3. rank textual relevance before authority and currentness boosts;
4. expand configured typed relations for at most the requested bounded depth;
5. select exact sections under count and byte budgets;
6. render paths, spans, kind, facts, currentness, relation path, and match
   reasons in text or versioned JSON.

Authority may break or improve a relevant result. It must never make an
unrelated document outrank a lexical match. Historical documents stay
available when directly relevant, but a related current authority wins by
default.

The query surface returns source evidence, not natural-language synthesis.
Agents remain responsible for reading the evidence and answering the user.

## Generic Baseline

A repository without `[docs_policy.graph]` gets:

- Markdown document and exact section nodes;
- ordinary Markdown links and local path references;
- full-text lexical retrieval;
- kind `document`, authority `0`, and currentness `unknown`;
- bounded `docs context` output with the same schema.

This baseline makes the feature useful outside Northstar. A profile adds local
meaning; it is not an enablement switch.

## Adoption Boundary

Installed skills and starters may supply example policy at setup time. They
are never consulted at query time. A skill upgrade cannot silently change a
consumer repository's authority weights or currentness rules. The selected
repository and selected catalog scope define which docs are indexed; query
work does not silently fan out to sibling catalogs. Optional, explicit
cross-repository source routing has its own opt-in boundary.

## Implementation and Ownership

- `effigy-manifest` parses, composes, and validates the profile.
- `effigy-codegraph` compiles the profile, extracts Markdown documents and
  exact heading sections, stores facts and typed links, tracks freshness, and
  performs bounded lexical retrieval in the shared graph database.
- `effigy-cli` owns grammar and help. The built-in docs command owns selected
  root resolution, rendering, exit behavior, and the
  `effigy.docs.context.v1` JSON payload.
- The selected repository owns the profile, the documents, and their factual
  status. The [user guide](../../guides/079-documentation-graph-profiles-and-context.md)
  owns invocation examples.

Profile content participates in graph freshness. Changing profile weights or
kinds reindexes the semantic layer even when Markdown bytes are unchanged.
Exact section spans come from the parser, not generated summaries. Search,
relations, and source text share the same database and lazy refresh lifecycle
as the code graph.

## Non-Goals

- a Northstar-only graph schema
- a second database, background daemon, or required MCP server
- embeddings or a remote vector service as a requirement
- model-generated summaries, tags, or inferred relations as authority
- replacing direct file reads, exact-token search, or the code graph
- crawling external websites or documentation outside the selected repository
- silently rewriting existing profiles when a starter or skill changes

## Architecture Acceptance

- a repository with no Northstar files can use the baseline and a custom profile
- a repository can choose arbitrary kind, field, and relation names
- Northstar behavior is expressed entirely by a committed profile
- current authoritative sections outrank related historical evidence without
  suppressing direct historical matches
- every returned claim carries exact repository provenance
- query budgets prevent unbounded context output
- the shared graph remains the only index and freshness authority
