# 079 - Documentation Graph Profiles And Context

`effigy docs context <QUERY>` returns small, exact, current documentation
evidence from the selected repository. This guide covers the repository-owned
profile that gives that evidence meaning, the adoption boundary between a
template and a committed profile, and the query shapes that pay off.

Canonical rules live in
[`../contracts/041-documentation-graph-profile-contract.md`](../knowledge/contracts/041-documentation-graph-profile-contract.md)
and
[`../architecture/024-repository-defined-documentation-graph.md`](../knowledge/architecture/024-repository-defined-documentation-graph.md).

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

A complete leading YAML frontmatter block (`---` on the first line through the
next standalone `---`) is metadata, not a section, including an empty body or a
body that begins with blank lines. Profile-configured field facts and labelled
relations inside that block stay extractable with their original spans.
Incomplete or later `---` delimiters keep ordinary Markdown heading behavior.

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
include = ["docs/knowledge/contracts/*.md"]
authority = 100
default-currentness = "current"

[docs_policy.graph.relations.next-task]
headings = ["Next move"]
```

Effigy reserves none of these names. A repository with no `docs/` directory, no
`Status:` convention, and no roadmap vocabulary configures its own tokens and
gets the same behavior; that neutrality is guarded by a test over the runtime
files and by the arbitrary-vocabulary fixture under
`tests/fixtures/docs-context-benchmark/generic-handbook/`.

Two rules catch most first-profile mistakes:

- **Kind globs must not overlap.** One document matches at most one kind, and an
  overlap is a profile error naming the path and both kinds. A single `*` never
  crosses a path separator; use explicit generation README, top-level task,
  archive, and template patterns instead of one broad roadmap glob.
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
- Repositories own their fields and currentness rules. Effigy's current profile in `docs/effigy.docs.toml` ranks knowledge, plan, guides, and triage for this repository. The committed profile is runtime authority; the starter is only an example.

## Example Queries

These are the shapes an agent actually needs. Each returns exact sections; read
the evidence and answer from it.

```sh
# 1. which contract governs this behavior
effigy docs context "documentation graph profile contract"

# 2. what is the architecture decision behind it
effigy docs context "repository defined documentation graph architecture"

# 3. what matters next in this repository
effigy docs context "Choose the next Effigy product outcome with Tom"

# 4. which user guide explains the surface
effigy docs context "documentation graph profiles and context guide"

# 5. what is the current release procedure
effigy docs context "Effigy release procedure"

# 6. what did an archived user guide say
effigy docs context "docs consistency sweep and changelog"

# 7. which section names this identifier
effigy docs context "catalog_tasks"
```

Effigy's current profile ranks [knowledge](../knowledge/README.md) as product authority and [the plan](../plan.md) as intent. Queue owns live task state, so a docs-context query does not answer which task is running. Historical process records remain in Git history.

The same shape in the arbitrary vocabulary of
`tests/fixtures/docs-context-benchmark/generic-handbook/`, where a live and a
retired document hold identical section text and only `State:` separates them:

```sh
# which procedure is the one in force
effigy docs context "escalation rota paging order" \
  --repo tests/fixtures/docs-context-benchmark/generic-handbook
```

That returns the `live` playbook at rank 1 and the `retired` bulletin at rank 2.

Shape 6 matters as much as shape 1. Default ranking prefers a current
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
  so `roadmap` in a repository of roadmaps carries no signal. That weighting is
  a ranking optimization only: if the weighted terms seed nothing, every term is
  re-enabled and seeding runs again, so it can never erase a query's only
  evidence. Name the thing you want, not the category it belongs to.
- **Exact identifiers stay whole.** A query token that joins alphanumeric runs
  with `_`, `-`, `.`, `::`, or `/` is kept as an exact term alongside its split
  words. Ranking credits whole-term containment of that identifier in section
  text, heading, path, or fields above split-word matches, and the match reason
  names the exact term. `catalog_tasks` therefore retrieves the section that
  contains that literal; `graph` still does not match `graphql`, and
  `catalog_tasks` does not match `catalog_tasks_v2`. Candidate recall still uses
  the shared full-text index with the split words; there is no second index and
  no tokenizer change.
- **Traversal remains reachable.** With at least two section slots, retrieval
  keeps the best lexical result first and reserves one slot for the best whole
  traversed result that fits the byte budget. Remaining slots follow the normal
  deterministic rank order. A one-section query remains lexical-only.

## Routing Across Repositories

One question often belongs to several checkouts at once. `--sources` asks all
of them in a single call and returns the answers grouped by repository:

```bash
effigy docs context "release gate policy" --sources ~/Dev/projects/portfolio.toml
effigy docs context "release gate policy" --sources ~/Dev/projects --only effigy
effigy --json docs context "release gate policy" --sources portfolio.toml
```

Membership is two-sided, and both halves are committed text. The portfolio file
names where to look:

```toml
# portfolio.toml - paths are relative to this file
[portfolio]
directories = ["."]
```

Each repository decides for itself whether it wants to be found:

```toml
# that repository's own effigy.toml
[docs_policy.sources]
share = true
front_doors = ["docs/README.md", "AGENTS.md"]
skill_roots = [".agents/skills"]
```

A child joins only if it is a git checkout, has a committed root `effigy.toml`
at `HEAD`, and declares
`share = true` **in that file**. Membership is read from the committed bytes of
the child's own root manifest and nothing else: no include, no
`effigy.local.toml` overlay, no bundle default. Dirty root-manifest edits cannot
opt a checkout in or out; commit the change first. Classification runs on
repositories that never opted in, so it must not compose them — composing would
let an uncommitted overlay grant membership, and would clone and cache a
declared bundle into a checkout the caller has no business writing to. A
repository that keeps the table only in an include is reported as `not-shared`,
and the next step names the file to move it to. Once a repository has opted in,
querying it uses its full manifest as usual. Enumeration is one level deep per named directory; it never
descends further, never follows a symlink out of the directory, and never
considers a hidden directory or one named `.paseo`, `worktrees`,
`node_modules`, or `target`. The handle is the directory name, and `--only`
selects on it. Duplicate handles across named directories fail before querying
any repository, even with `--only`; give the checkout directories distinct
names. Passing a
directory to `--sources` is the same as a portfolio
naming that one directory. There are no globs and no unknown keys: both files
fail to parse rather than quietly widening.

Each repository is answered through the same single-repository retrieval, with
its own graph, lock, freshness, and full section and byte budget. Execution is
sequential and there is no shared index or cache. Results stay in their own
block, ranked from 1 within it: rankings are never merged and one repository's
authority is never compared with another's, because authority is repository
policy and two repositories do not share one policy.

### Statuses

Every repository the portfolio can reach appears in the report, healthy or not,
and every non-`ok` status carries a next step.

| status | meaning |
| --- | --- |
| `ok` | answered with at least one section |
| `empty` | answered, and nothing in it matched |
| `stale` | answered from an index that is behind or carries failed paths |
| `timeout` | did not answer inside `EFFIGY_GRAPH_TIMEOUT_MS` |
| `not-shared` | present, and never opted in |
| `missing` | the named directory or checkout is absent |
| `invalid` | not a checkout, or its directory or committed manifest could not be read; directory listing errors carry the I/O reason |
| `disallowed` | an `--only` handle that resolved to nothing |

The call exits 0 when at least one repository is `ok` or `empty`, so a
degraded neighbour never hides a healthy one. It fails only when none answered,
and the failure still lists every status. A missing or unparsable portfolio
file is a usage error: a caller that named a portfolio wants that portfolio.

### Source identity

Every repository block carries the checkout's current HEAD and the HEAD its
index was built from, and every result carries `content_identity`: `committed`
when the file matches HEAD, `working-tree` otherwise. Identity is never
optimistic — if Git cannot answer, the exact path is absent from `HEAD`, or its
status is uncertain, the excerpt is reported as working-tree content.

## Freshness

The documentation graph shares the code graph database, lock, health, and lazy
refresh under `.effigy/graph/`. There is no second index and no daemon. A
normalized profile fingerprint joins the freshness identity, so editing the
profile refreshes semantic records even when no Markdown file changed.

Lazy refresh shares the graph command's wall-clock policy through
`EFFIGY_GRAPH_TIMEOUT_MS` (default 120000 ms; `0` disables the bound). Cold and
stale rebuilds announce progress on stderr. A timeout returns the shared
`effigy.graph.timeout.v1` detail with graph health, the phase the bound expired
in, and recovery guidance; JSON stdout remains a valid standard command
envelope. See [`076`](076-code-graph-and-agent-workflows.md) for the phase
names and what each one means.

## Measuring Retrieval Quality

`effigy perf:docs-context-benchmark` replays a predeclared corpus over the
generic fixture and this repository, and fails if a declared live authority
falls outside the top three, a declared historical rival outranks it, a directly
named historical source is not retrieved, an unrelated high-authority document
enters a report, or a fixture no-match query returns anything. Empty-result
cases run only against fixture corpora; a live-target empty case is rejected
before the matrix executes.

A third target replays cross-repository routing over
`tests/fixtures/docs-context-benchmark/portfolio`, with a frozen case for every
status, for grouping, for source identity, and for both exit rules. It copies
the fixture into the report directory and turns three children into real git
checkouts, because a dirty file and a duplicate single-valued field cannot
exist in a clean committed tree.

The corpus, expected sources, and pass criteria are frozen in
`scripts/benchmark-docs-context.rhai` and committed before each run, with the
freeze history recorded in the file. Reports land under
`.effigy/perf/docs-context-benchmark/`.

## Related Guides

- [`025-command-reference-matrix.md`](025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](026-json-payload-examples.md)
- [`029-docs-qa-checklist-and-validation.md`](029-docs-qa-checklist-and-validation.md)
- [`047-agent-and-cross-repo-adoption.md`](047-agent-and-cross-repo-adoption.md)
- [`056-northstar-effigy-consumer-repo-contract.md`](056-northstar-effigy-consumer-repo-contract.md)
- [`076-code-graph-and-agent-workflows.md`](076-code-graph-and-agent-workflows.md)

## Next Step

Adopt a profile in one consumer repository, run
`effigy docs context` against the example query shapes above, and tune the
kind authority weights until the answers you expect lead. Keep the vocabulary
yours.
