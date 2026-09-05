# 041 - Documentation Graph Profile Contract

Status: active
Owner: Platform maintainers
Architecture: [`024`](../architecture/024-repository-defined-documentation-graph.md)
Roadmap: [`g08.035`](../roadmaps/g08/035-repository-defined-documentation-graph.md)
Spec: [`108`](../specs/archive/108-documentation-graph-profiles-strict-lane.md)

## Purpose

Define a repository-neutral documentation graph whose semantic shape is owned
by the selected repository. The contract covers the optional manifest profile,
deterministic Markdown facts and relations, bounded context retrieval, and the
Northstar adoption boundary.

## Baseline Contract

`effigy docs context` must work when `[docs_policy.graph]` is absent. Baseline
mode indexes Markdown documents, exact sections, ordinary links, local path
references, and lexical source. Results use kind `document`, authority `0`, and
currentness `unknown`.

No baseline behavior may assume a `docs/` directory, Northstar headings,
numbered files, status values, or roadmap vocabulary.

## Manifest Grammar

The optional graph profile lives under `[docs_policy.graph]`.

```toml
[docs_policy.graph]
roots = ["README.md", "docs"]

[docs_policy.graph.fields.<FIELD>]
labels = ["Status"]
cardinality = "one" # one | many

[docs_policy.graph.currentness]
field = "status"
current = ["active", "ready"]
historical = ["complete", "archived"]

[docs_policy.graph.kinds.<KIND>]
include = ["docs/contracts/*.md"]
exclude = []
authority = 100
default-currentness = "unknown" # current | historical | unknown

[docs_policy.graph.relations.<RELATION>]
labels = ["Contract", "Contracts"]
headings = ["Contracts"]
```

Rules:

- `roots` is a non-empty list of repository-relative files or directories.
- Field, kind, and relation names are non-empty stable tokens and unique within
  their map.
- A field has at least one non-empty label. Labels compare case-insensitively
  after trimming surrounding whitespace.
- `cardinality` defaults to `one`; duplicate single-valued facts are an indexing
  diagnostic, not a last-value-wins rule.
- `currentness.field` must name a declared field. Its value sets are non-empty,
  disjoint, and compare case-insensitively.
- A kind has at least one include glob. Excludes are applied after includes.
- `authority` is an integer from `0` through `100` and defaults to `0`.
- One document may match at most one kind. An overlap is a profile error that
  names the path and matching kinds.
- A relation has at least one label or heading selector. Links matching either
  selector produce an outgoing edge named by the relation token.
- Unknown keys fail manifest parsing. Invalid profiles fail before indexing or
  query work begins.
- Paths must stay inside the selected repository after normalization. Symlink
  escape is rejected.

The implementation may accept snake-case aliases where existing docs-policy
configuration already does, but generated reference documentation must show
one canonical spelling.

## Markdown Extraction

- A document node spans the complete file.
- A complete leading YAML frontmatter block is metadata, not a heading or
  section. Its configured field facts and labelled relations remain
  extractable with original source spans.
- A section starts at its heading and ends before the next heading of the same
  or higher level, or at end of file.
- Nested headings belong to the enclosing section while retaining their own
  exact section nodes.
- Setext and ATX headings supported by the Markdown parser follow the same rule.
- Field facts come only from plain `Label: value` lines outside code fences.
- Relation links come only from configured heading sections or configured
  labelled metadata lines.
- Relative link targets resolve from the source document. Fragments are
  preserved. External URLs remain unresolved external targets and are not
  traversed.
- Every node, fact, edge, and diagnostic carries exact path and source span.
- Extraction is deterministic for identical profile and repository bytes.

Missing fields, unknown field values, unresolved local links, and unmatched
documents remain visible as facts or diagnostics. Effigy must not invent
values from prose.

## Currentness And Authority

Currentness resolves from the configured currentness field, then the kind's
default, then `unknown`. A field value outside configured sets stays `unknown`.

Authority is repository policy, not semantic relevance. Retrieval must apply
it only after a result has a lexical or traversed relationship to the query.
An authority weight cannot introduce an otherwise unrelated result.

Historical results remain eligible. Default ranking prefers a relevant current
document over a similarly relevant historical document, while a query that
directly names historical material may still rank it first.

## Command Contract

```text
effigy docs context <QUERY> [--max-sections <N>] [--max-bytes <N>]
                            [--max-hops <N>] [--sources <PATH>]
                            [--only <HANDLE>]
```

Standard leading `--repo <PATH>` and `--json` behavior applies.

- defaults: `max-sections = 8`, `max-bytes = 24000`, `max-hops = 1`
- hard limits: `max-sections <= 32`, `max-bytes <= 100000`, `max-hops <= 3`
- all budgets must be positive
- a query refreshes the shared graph through the existing freshness path
- lazy refresh uses the shared graph wall-clock policy,
  `EFFIGY_GRAPH_TIMEOUT_MS`; `0` disables the bound
- a cold or stale refresh emits one progress notice on stderr before the
  repository walk; text and JSON stdout remain contract-pure
- an empty or whitespace-only query is a usage error
- no-match is a successful empty report, not a fallback to arbitrary files

Text output is concise evidence suitable for an agent context window. JSON uses
a versioned `effigy.docs.context.v1` payload inside the standard command
envelope. With `--sources`, it uses the distinct grouped
`effigy.docs.context.sources.v1` payload; each repository retains its own
ranking, authority, currentness, freshness, and identity.

Minimum JSON fields:

- query, repo root, profile state, profile fingerprint, and graph freshness
- requested and applied budgets
- result path, heading, kind, authority, currentness, exact span, and source
- extracted fields and typed relation path
- lexical/traversal match reasons and truncation state
- diagnostics and actionable next steps when profile or links are unhealthy

The payload contains source evidence, not an answer or generated summary.

## Retrieval Rules

1. Restrict lexical candidates to configured roots, or all Markdown in baseline
   mode.
2. Seed from exact path, heading, field, and source-text matches.
3. Rank textual relevance before currentness and authority.
4. Traverse only configured relation edges up to `max-hops`.
5. Deduplicate overlapping sections from the same document.
6. Apply section count and byte budgets deterministically. When
   `max-sections >= 2`, preserve the highest-ranked lexical result and reserve
   one slot for the highest-ranked traversed candidate that fits whole. Fill
   remaining slots with the existing rank order. With `max-sections = 1`, keep
   the single best lexical result.
7. Preserve at least the matched source span or omit the result with an explicit
   budget diagnostic; never emit a misleading partial line.

Tie-breaking uses stable path, heading position, and relation order. Filesystem
iteration order must not affect output.

## Freshness

The documentation graph shares the code graph database, lock, health, and lazy
refresh behavior. A normalized profile fingerprint joins the freshness identity.
A profile edit must refresh semantic documentation records even when Markdown
files did not change.

The graph command and every lazy-refresh consumer share one time-budget parser,
bounded execution path, typed timeout detail, health snapshot, and recovery
guidance. A docs query must not introduce a second refresh or timeout model.

No second index directory or background service is allowed.

## Northstar Boundary

Northstar may ship a profile template through a skill, starter, or init asset.
The template becomes active only after it is committed into the consumer
repository. Runtime behavior reads the consumer manifest and never an installed
skill directory.

Northstar-specific kind names, paths, statuses, and relations belong in that
template. They must not appear as fallback rules in generic extraction or
retrieval code.

## Acceptance

- baseline retrieval works in a repository with Markdown and no docs profile
- a non-Northstar fixture defines arbitrary kinds, fields, and relations
- invalid roots, escaped paths, kind overlap, duplicate single-valued facts,
  and invalid currentness references produce deterministic diagnostics
- heading spans isolate exact sections rather than the complete file
- a profile-only edit refreshes semantic records
- current authoritative evidence wins relevant ties over historical evidence
- direct historical queries still retrieve the named historical section
- count, byte, and hop budgets are enforced in text and JSON
- every result has exact provenance and a machine-readable match reason
- a Northstar profile works from committed consumer config with the skill absent

## Out Of Scope

- embeddings, semantic vector search, or remote inference
- LLM-authored graph nodes, summaries, or relations
- external-site crawling
- graph mutation from query commands
- profile inheritance from an installed skill at runtime
- replacing code-navigation commands or ordinary exact-text search
- a daemon, required MCP integration, or editor-specific protocol

## Drift Triggers

Update this contract with changes to profile grammar, source-routing grammar,
matching precedence,
section boundaries, currentness, authority use, query budgets, JSON shape,
freshness identity, or Northstar runtime independence.

## Next Task

Cards `1101` and `1102` are complete. Cards `1113` through `1115` completed
the bounded latency, exact-identifier, and opt-in cross-repository routing
lanes. The single-repository contract and refresh path remain authoritative;
cross-repository results are grouped and use the distinct sources payload.
