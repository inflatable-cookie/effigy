# 079 - Documentation Graph Profiles And Context

`effigy docs context <QUERY>` returns small, exact, current documentation
evidence from the selected repository. This guide covers the repository-owned
profile that gives that evidence meaning, the adoption boundary between a
template and a committed profile, and the query shapes that pay off.

Canonical rules live in
[`../contracts/041-documentation-graph-profile-contract.md`](../contracts/041-documentation-graph-profile-contract.md)
and
[`../architecture/024-repository-defined-documentation-graph.md`](../architecture/024-repository-defined-documentation-graph.md).

## Vision Alignment

- Primary tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Target envelope: an agent retrieves the governing contract, architecture, or
  planning section in one bounded call instead of reconstructing the docs system
  from file layout.

## What The Command Returns

Source evidence, not an answer. Every result is an exact section slice with its
path, heading anchor, line and byte span, repository-declared kind, authority,
currentness, extracted fields, typed relation path, and a machine-readable match
reason. Nothing is summarized, inferred, or generated.

```sh
effigy docs context "release gates"
effigy docs context "documentation graph profile" --max-sections 4 --max-bytes 8000
effigy --json docs context "graph freshness" --max-hops 2
```

Budgets are `--max-sections` (default 8, maximum 32), `--max-bytes` (default
24000, maximum 100000), and `--max-hops` (default 1, maximum 3). A section that
does not fit the byte budget is omitted whole and named in `truncation.reasons`;
no partial section is ever emitted.

## Baseline Mode Needs No Configuration

With no `[docs_policy.graph]` block the command still works. It indexes every
Markdown file, exact sections, ordinary links, and local path references, and
reports kind `document`, authority `0`, and currentness `unknown`. A profile
adds local meaning; it is not an enablement switch.

## The Repository-Owned Profile

A profile names five things, all of them repository-defined tokens:

| Concept | What it declares |
| --- | --- |
| `roots` | which files and directories are in scope for retrieval |
| `fields` | which `Label: value` metadata lines become facts |
| `currentness` | which values of one field mean live, and which mean history |
| `kinds` | which path globs form a document family, and its authority weight |
| `relations` | which labelled links and heading sections become typed edges |

```toml
[docs_policy.graph]
roots = ["README.md", "docs"]

[docs_policy.graph.fields.status]
labels = ["Status"]
cardinality = "one"

[docs_policy.graph.currentness]
field = "status"
current = ["active", "ready"]
historical = ["complete", "archived"]

[docs_policy.graph.kinds.contract]
include = ["docs/contracts/*.md"]
authority = 100
default-currentness = "current"

[docs_policy.graph.relations.next-task]
headings = ["Next Task"]
```

Effigy reserves none of these names. A repository with no `docs/` directory, no
`Status:` convention, and no roadmap vocabulary configures its own tokens and
gets the same behavior; that neutrality is guarded by a test over the runtime
files and by the arbitrary-vocabulary fixture under
`tests/fixtures/docs-context-benchmark/generic-handbook/`.

Two rules catch most first-profile mistakes:

- **Kind globs must not overlap.** One document matches at most one kind, and an
  overlap is a profile error naming the path and both kinds. A single `*` never
  crosses a path separator, which is what keeps `docs/roadmaps/*/*.md` disjoint
  from `docs/roadmaps/*/batch-cards/*.md`.
- **Authority is policy, not relevance.** It orders results that already match
  lexically or over a traversed relation. It can never pull an unrelated
  document into a report.

## Adoption: A Template Is Copied, Never Inherited

Northstar is one profile, not a built-in ontology. `effigy init northstar`
materializes the profile into the consumer `effigy.toml`, and from that moment
the committed bytes are the only runtime authority.

- Effigy reads the selected repository's manifest and nothing else at query
  time. It does not consult an installed skill directory, a starter, or a
  template cache.
- Upgrading Effigy, or updating an installed agent skill, cannot silently
  reinterpret an existing repository.
- Adopting a newer template is an explicit merge: run
  `effigy init northstar --dry-run`, read the emitted block, and take the parts
  you want.
- Deviating is expected. Effigy's own profile in `docs/effigy.docs.toml` sets
  `cardinality = "many"` for `status` and `owner` because its roadmap and
  architecture documents legitimately carry per-section metadata lines; the
  starter ships `"one"`. Currentness then resolves from the first occurrence in
  file order, which is the document header.

## Example Queries

These are the five shapes an agent actually needs. Each returns exact sections;
read the evidence and answer from it.

```sh
# which contract governs this behavior
effigy docs context "documentation graph profile contract"

# what is the architecture decision behind it
effigy docs context "repository defined documentation graph architecture"

# what is the active planning lane
effigy docs context "active strict lane spec set"

# what is the next task in that lane
effigy docs context "next task" --max-sections 4

# what did we decide before, and why
effigy docs context "bounded documentation context query closeout evidence"
```

The last shape matters as much as the first. Default ranking prefers a current
document over a *similarly relevant* historical one, but a query that names
historical material directly still retrieves it — an archived guide asked for by
its own title ranks first, above the live guide that superseded it, because
relevance ranks before currentness.

## Reading The Ranking

Order is: hop distance, then textual relevance, then currentness, then
authority, then heading depth, then a stable path and span tie-break. Two
consequences are worth knowing before you tune a query:

- **Relevance leads.** A vague query built from words that appear everywhere is
  ordered by relevance noise. A term reaching more than half of a corpus of at
  least eight scoped documents is dropped from scoring as ordinary vocabulary,
  so `roadmap` in a repository of roadmaps carries no signal. Name the thing you
  want, not the category it belongs to.
- **Traversal supplements thin evidence.** Traversed results rank after every
  direct lexical result, so on a large corpus the section budget is usually
  spent before a one-hop result can be selected. Raise `--max-sections`, or
  narrow the query, when you specifically want relation evidence.

## Freshness

The documentation graph shares the code graph database, lock, health, and lazy
refresh under `.effigy/graph/`. There is no second index and no daemon. A
normalized profile fingerprint joins the freshness identity, so editing the
profile refreshes semantic records even when no Markdown file changed.

The first query in a fresh checkout pays a full cold index and is currently
unbounded; see `PAPERCUTS.md`.

## Measuring Retrieval Quality

`effigy perf:docs-context-benchmark` replays a predeclared corpus over the
generic fixture and this repository, and fails if a declared live authority
falls outside the top three, a declared historical rival outranks it, a directly
named historical source is not retrieved, an unrelated high-authority document
enters a report, or a no-match query returns anything.

The corpus, expected sources, and pass criteria are frozen in
`scripts/benchmark-docs-context.rhai` and committed before each run, with the
freeze history recorded in the file. Reports land under
`.effigy/perf/docs-context-benchmark/`.

## Related Guides

- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`047-agent-and-cross-repo-adoption.md`](./047-agent-and-cross-repo-adoption.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](./056-northstar-effigy-consumer-repo-contract.md)
- [`076-code-graph-and-agent-workflows.md`](./076-code-graph-and-agent-workflows.md)

## Next Step

Adopt a profile in one consumer repository, run
`effigy docs context` against the five example query shapes above, and tune the
kind authority weights until the answers you expect lead. Keep the vocabulary
yours.
