# Repository-Defined Documentation Graph Architecture

Status: active
Updated: 2026-08-29
Roadmap: [`g08.035`](../roadmaps/g08/035-repository-defined-documentation-graph.md)
Contract: [`041`](../contracts/041-documentation-graph-profile-contract.md)
Spec: [`108`](../specs/108-documentation-graph-profiles-strict-lane.md)

## Purpose

Effigy's code graph already indexes Markdown documents, headings, links, code
fences, path references, and full-text source. That is enough for broad search,
but not enough to answer documentation questions with reliable authority or
currentness. A model can find the right words and still receive a completed
roadmap, archived spec, or incidental handoff before the live contract.

The documentation graph adds a repository-defined semantic layer to the
existing graph. Effigy owns generic mechanics. Each repository owns the names
and paths that make its documentation authoritative. Northstar is one supplied
profile, not a runtime dependency or built-in ontology.

## Vision Alignment

- Primary tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Target envelope: agents retrieve bounded, current, authoritative project
  evidence without reconstructing the documentation system from file layout.
- Vision target delta: graph-assisted navigation expands from code ownership
  to repository-defined documentation semantics without adding a daemon, MCP
  requirement, or generated summaries as authority.

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
- Ship a Northstar profile through adoption assets. The profile must be copied
  into the consumer repository so behavior does not depend on an installed
  agent skill.

## Current System Inventory

| Surface | Current capability | Required change |
| --- | --- | --- |
| `effigy-manifest` | Typed `[docs_policy.indexes]` and `[docs_policy.next_actions]` | Own typed graph-profile grammar and validation |
| Markdown indexer | Files, headings, links, code fences, local path references | Emit exact section spans and profile-backed facts/relations |
| Graph storage | Files, symbols, edges, references, diagnostics, FTS source | Reuse these primitives; migrate only if typed facts cannot remain lossless |
| Graph freshness | Lazy refresh, health, incremental indexing | Include profile content in freshness identity |
| Graph queries | Search, context, callers, impact, explore | Reuse lexical seeding and traversal primitives |
| Docs commands | Deterministic documentation checks | Add bounded documentation context retrieval |
| Northstar starter | Repo-owned docs-policy checks | Supply a committed graph profile example |

The first implementation must correct the current Markdown heading span, which
uses the whole document. Section evidence needs the heading start through the
next heading of the same or higher level, with exact line and byte positions.

## Repository-Owned Profile

The planned shape is deliberately small:

```toml
[docs_policy.graph]
roots = ["README.md", "docs"]

[docs_policy.graph.fields.status]
labels = ["Status"]
cardinality = "one"

[docs_policy.graph.fields.owner]
labels = ["Owner"]
cardinality = "one"

[docs_policy.graph.currentness]
field = "status"
current = ["active", "ready", "strict-ready", "draft"]
historical = ["complete", "archived", "superseded"]

[docs_policy.graph.kinds.contract]
include = ["docs/contracts/*.md"]
authority = 100

[docs_policy.graph.kinds.archived-spec]
include = ["docs/specs/archive/*.md"]
authority = 10
default_currentness = "historical"

[docs_policy.graph.relations.contract]
labels = ["Contract", "Contracts"]

[docs_policy.graph.relations.next-task]
headings = ["Next Task"]
```

Names below `fields`, `kinds`, and `relations` are repository-defined tokens.
Effigy must not reserve Northstar names such as `roadmap`, `ready-card`, or
`contract`. Profiles may use any valid names that satisfy contract `041`.

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
2. the matched kind's `default_currentness`;
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

## Northstar Adoption Boundary

The Northstar profile may define kinds such as contract, architecture, guide,
vision, roadmap, ready card, log, handoff, and archived spec, plus relations
such as contract, roadmap, evidence, supersedes, and next task.

The profile can originate in the Northstar skill or Effigy's Northstar starter,
but installation or init must materialize it into the consumer `effigy.toml`.
After that point the consumer copy is runtime authority. Updating a skill must
not silently reinterpret an existing repository.

## Ownership

- `effigy-manifest` owns profile types, aliases, validation, and composition.
- `effigy-codegraph` owns profile compilation, exact Markdown structure,
  semantic facts/relations, freshness, retrieval, and typed reports.
- `effigy-cli` owns grammar and help.
- the built-in docs command shell owns root selection, rendering, JSON envelope,
  and exit behavior.
- starters and guides own profile examples and adoption guidance.
- consumer repositories own their committed profile and documentation content.

## Non-Goals

- a Northstar-only graph schema
- a second database, background daemon, or required MCP server
- embeddings or a remote vector service in the first lane
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
