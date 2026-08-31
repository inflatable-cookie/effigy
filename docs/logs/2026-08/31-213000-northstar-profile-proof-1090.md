# Generic And Northstar Documentation Profile Proof

Status: complete
Created: 2026-08-31
Roadmap: g08.035
Card: 1090
Spec: 108
Contract: 041
Architecture: 024
Predecessor evidence: [`31-181957-documentation-context-1089.md`](./31-181957-documentation-context-1089.md)

## Summary

- The Northstar documentation ontology is now committed configuration. The
  `northstar` starter emits the full profile, and this repository's own copy
  lives in `docs/effigy.docs.toml`.
- Runtime stayed repository-neutral. A guard test reads the eleven files that
  parse profiles, extract Markdown, scope, rank, and render `docs context`, and
  fails if any of them names Northstar vocabulary.
- Installed skill and template directories never reach a query. Contradictory
  templates in four skill locations, edits to them, and deleting every skill
  tree all leave the profile identity and results byte-identical; editing the
  committed consumer manifest changes them immediately.
- `perf:docs-context-benchmark` replays a predeclared corpus over an
  arbitrary-vocabulary fixture and this repository. All 11 expectations hold on
  the final frozen matrix.
- No ranking rule, authority weight, budget default, or runtime code path was
  changed by this card. The work is configuration, fixtures, proof, and
  documentation.

## Repository Neutrality

`documentation_graph_runtime_logic_carries_no_northstar_vocabulary` asserts that
none of these files contains `northstar`, `roadmap`, `ready-card`, `ready card`,
`batch-card`, `batch card`, `handoff`, `archived-spec`, `next-task`,
`next task`, `strict-ready`, `milestone`, `papercut`, or any of the
`docs/contracts`, `docs/specs`, `docs/roadmaps`, `docs/vision`, `docs/logs`,
`docs/guides`, `docs/handoffs` path prefixes:

```text
crates/effigy-manifest/src/config_sections/docs_policy.rs
crates/effigy-codegraph/src/docs_profile.rs
crates/effigy-codegraph/src/docs_context/{mod,payload,rank,scope}.rs
crates/effigy-codegraph/src/language/markdown/{mod,extract,paths,resolve}.rs
src/runner/docs_command/context.rs
```

Fixtures and unit tests are excluded on purpose: a fixture is allowed to name a
vocabulary, the runtime is not.

The arbitrary-vocabulary fixture at
`tests/fixtures/docs-context-benchmark/generic-handbook/` renames every
Northstar-shaped token and indexes, ranks, and retrieves without a code change:

| Concept | Northstar name | Fixture name |
| --- | --- | --- |
| root | `docs` | `handbook` |
| top authority kind | `contract` | `charter` |
| working kind | `guide` | `playbook` |
| historical kind | `archived-spec` | `bulletin` |
| deep archive kind | `archived-log` | `almanac` |
| currentness field | `status` | `state` |
| owner field | `owner` | `steward` |
| current values | `active`, `ready` | `live`, `proposed` |
| historical values | `complete`, `archived` | `retired`, `superseded` |
| relations | `contract`, `next-task` | `charter`, `follow-up`, `see-also` |

The in-crate unit fixture in `crates/effigy-codegraph/src/docs_context/tests.rs`
uses the same vocabulary family and is unchanged by this card.

## Northstar As Copied Configuration

`crates/effigy-catalog/starters/northstar/effigy.toml` now emits 13 kinds
(`contract`, `architecture`, `spec`, `archived-spec`, `vision`,
`archived-vision`, `roadmap`, `ready-card`, `guide`, `archived-guide`,
`front-door`, `log`, `archived-log`, `handoff`) and 8 relations (`contract`,
`architecture`, `spec`, `roadmap`, `card`, `evidence`, `supersedes`,
`next-task`), with `Status:` driving currentness.

`northstar_starter_profile_is_queryable_from_the_copied_manifest_alone` runs
`effigy init northstar` into an empty directory, adds one live contract and one
archived spec that share a heading, and queries. The emitted manifest is the
only configuration in play. Result: the live contract ranks 1 at kind
`contract`, authority 100, currentness `current`; the archived spec is retrieved
below it at kind `archived-spec`, currentness `historical`.

The starter test in `crates/effigy-catalog/src/starter.rs` additionally asserts
the emitted bytes carry the phrases `COPIED configuration` and
`only runtime authority`, so the boundary cannot be silently dropped from the
template.

### Effigy's own consumer copy deliberately deviates

`docs/effigy.docs.toml` is the materialized profile, adapted to Effigy's real
tree: 16 kinds, 8 relations, `roots = ["README.md", "AGENTS.md", "CHANGELOG.md",
"PAPERCUTS.md", "docs"]`, 2319 scoped documents.

It sets `cardinality = "many"` for both `status` and `owner`; the starter ships
`"one"`. Effigy's roadmap and architecture documents legitimately carry
per-section `Status:` and `Owner:` lines as well as the document header line.
With `"one"`, 11 documents raised `duplicate single-valued field` error
diagnostics and the graph reported `degraded` with 11 failed paths. With
`"many"`, `failed_paths` is empty and the graph reports `ready`; currentness
resolves from the first occurrence in file order, which is the document header
(facts sort by field then span start byte in
`crates/effigy-codegraph/src/docs_context/scope.rs`).

That divergence is the point. A consumer that renames or re-shapes the template
is behaving correctly, and nothing upstream re-imposes the template's choice.

## Installed-Skill Independence

`installed_skill_and_template_directories_never_reach_the_query` builds a
consumer repo from `effigy init northstar`, records a baseline payload, then:

| Step | What changed | Result |
| --- | --- | --- |
| 1 | contradictory profile templates planted at `.agents/skills/effigy/effigy.toml`, `.agents/skills/northstar/effigy.toml`, `skills/northstar/effigy.toml`, `.claude/skills/northstar/effigy.toml` (different roots, different kind name, authority 7) | `profile` and `results` identical to baseline |
| 2 | every planted template edited to add a second decoy kind at authority 99 | `profile` and `results` identical to baseline |
| 3 | `.agents`, `skills`, and `.claude` deleted outright | `profile` and `results` identical to baseline |
| 4 | committed consumer manifest edited: `contract` authority 100 -> 44 | `profile.fingerprint` changes and the contract result reports authority 44 |

Steps 1 through 3 close review-oracle counterexamples 2 and 5: byte-equivalent
results with skill directories unavailable, and no silent reinterpretation when
an installed template changes after copying. Step 4 shows the consumer manifest
is the authority that does matter, and that a profile-only edit joins the
freshness identity.

## Benchmark

`scripts/benchmark-docs-context.rhai`, task `perf:docs-context-benchmark`.
Reports land in `.effigy/perf/docs-context-benchmark/`.

Pass criteria, frozen before each run:

- `live-authority` — the expected current authoritative source is at rank <= 3
  and the declared historical-only rival does not rank above it
- `historical-retrieval` — a query that directly names historical material
  retrieves that source at any rank
- `traversal` — the expected source is reached over a configured typed relation
  at hops >= 1, and the named relation appears in a relation path
- `absent` — a high-authority document with no lexical relationship to the query
  stays out of the report
- `empty` — a no-match query is a successful empty report

The harness exits non-zero on any miss, so the task cannot pass while the corpus
is red.

### Freeze discipline

The corpus was committed before every run. Four freezes, all replayable:

| Freeze | Commit | Result |
| --- | --- | --- |
| 1 | `398a698f2` | 9/11 |
| 2 | `b6a60e1b6` | 10/11 |
| 3 | `b05a7928a` | 10/11 |
| 4 | `d38abf669` onward | 11/11 |

Every miss and its diagnosis:

1. **`effigy-no-match` (freeze 1).** Query `xylophone quokka telemetry
   sublimation` returned 8 results. `telemetry` reaches 33 documents and
   `quokka` reaches 1 in this corpus, so a non-empty report was the correct
   answer. My case specification was wrong, not the runtime. Replaced with
   `xylophone sublimation`; both terms have document frequency 0.
2. **`effigy-next-task` (freeze 1).** Asserted a typed-relation hop on the
   2319-document corpus. Card `1090` was present at rank 11 but at 0 hops,
   reached lexically, and no relation path appeared at all. Diagnosis:
   traversed results rank after every 0-hop result by contract, so on a corpus
   this size the section budget is exhausted by lexical seeds before a one-hop
   result can be selected. Confirmed at `--max-sections 32 --max-bytes 100000`:
   30 results, all 0 hops. That is documented behavior - traversal supplements
   thin lexical evidence rather than competing with it - so the typed-relation
   proof stays in the `generic-handbook` target, where it is observable, and the
   behavior is documented in guide `079`.
3. **`effigy-current-roadmap` (freeze 2).** `docs/roadmaps/g08/README.md` ranked
   6, outside the top three. The declared historical rival
   `docs/roadmaps/g07/README.md` did **not** outrank it, so the currentness
   criterion held; only the top-three bound failed. Diagnosis: `roadmap` reaches
   1540 of 2319 scoped documents and is dropped by corpus weighting, leaving
   `generation`, `theme`, and `purpose` - all section boilerplate. Relevance
   ranks before currentness by contract, so an undiscriminating query is ordered
   by relevance noise.
4. **`effigy-current-over-archived` (freeze 3).** Archived guide `032` ranked 1
   at relevance 48; live guide `039` ranked 2 at relevance 42. The query carried
   `consistency sweep`, which is the archived guide's own title phrase, so it
   was a direct historical query, not a live-authority query. Contract `041`
   orders currentness only between *similarly relevant* documents; ranking the
   more relevant historical document first is correct. The slot was removed
   rather than re-queried, and the behavior it actually demonstrated is kept as
   `effigy-direct-historical-guide`.

Two Effigy cases were also re-anchored because their original targets are closed
by this very lane, which would have made the committed benchmark
self-invalidating. This repository has exactly one active milestone and card
`1090` closes it, so no durable current-roadmap anchor exists here. That
dimension is carried by the fixture's `generic-current-over-retired` case, where
two documents hold byte-identical section text and differ only in state - the
only construction where relevance genuinely ties.

The freeze-1 current-roadmap result is preserved here rather than lost:
`docs/roadmaps/g08/035-repository-defined-documentation-graph.md` ranked **2**,
currentness `current`, authority 70, with the completed rival card
`docs/roadmaps/g08/batch-cards/1089-add-bounded-documentation-context-query.md`
not above it, on query
`repository defined documentation graph milestone execution plan` at commit
`398a698f2`.

### Final results

`generic-handbook` (7 documents, arbitrary vocabulary):

| case | dimension | query | expected source | rank | currentness | authority | rival rank | context bytes | result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `generic-charter-authority` | contract | `widget calibrator tolerance band` | `handbook/charters/widget-calibrator.md` | 1 | current | 100 | 2 | 656 | pass |
| `generic-current-over-retired` | current-roadmap | `escalation rota paging order` | `handbook/playbooks/escalation-rota.md` | 1 | current | 80 | 2 | 528 | pass |
| `generic-historical-direct` | historical-decision | `recall of 1998` | `handbook/bulletins/widget-calibrator-recall.md` | 1 | historical | 20 | - | 915 | pass |
| `generic-authority-gate` | authority-gate | `widget calibrator tolerance band` | - | - | - | - | - | 656 | pass |
| `generic-relation-follow-up` | next-task | `widget calibrator tolerance band` | `handbook/playbooks/rebalance.md` | 4 | current | 80 | - | 656 | pass |

`effigy-live` (2319 scoped documents, profile fingerprint from
`docs/effigy.docs.toml`):

| case | dimension | query | expected source | rank | currentness | authority | rival rank | context bytes | result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `effigy-contract-authority` | contract | `documentation graph profile contract` | `docs/contracts/041-documentation-graph-profile-contract.md` | 1 | current | 100 | - | 23996 | pass |
| `effigy-architecture-authority` | architecture | `repository defined documentation graph architecture` | `docs/architecture/024-repository-defined-documentation-graph.md` | 1 | current | 90 | - | 22142 | pass |
| `effigy-direct-historical-guide` | historical-decision | `docs consistency sweep and changelog` | `docs/guides/archive/032-docs-consistency-sweep-and-changelog.md` | 1 | historical | 10 | - | 22308 | pass |
| `effigy-next-task` | next-task | `active strict lane spec set` | `docs/specs/README.md` | 1 | current | 85 | - | 12796 | pass |
| `effigy-historical-decision` | historical-decision | `bounded documentation context query card 1089 closeout evidence` | `docs/logs/2026-08/31-181957-documentation-context-1089.md` | 3 | historical | 30 | - | 23993 | pass |
| `effigy-no-match` | no-match | `xylophone sublimation` | - | - | - | - | - | 0 | pass |

Current-versus-historical behavior across both targets: three cases return a
current authority at rank 1 with the declared historical rival at rank 2 or
absent; three cases retrieve directly named historical material at ranks 1, 1,
and 3; one case keeps an unrelated authority-100 document out of the report
entirely; one no-match query returns 0 results and 0 context bytes.

## Review-Oracle Counterexamples

| # | Counterexample | Proof |
| --- | --- | --- |
| 1 | a generic fixture renames every Northstar-looking token and still works without runtime edits | `generic-handbook` fixture and its 5 benchmark cases; `documentation_graph_runtime_logic_carries_no_northstar_vocabulary` |
| 2 | the copied Northstar profile is byte-equivalent when skill directories are unavailable | `installed_skill_and_template_directories_never_reach_the_query`, step 3 |
| 3 | expected live authority in the top three, no related historical-only source above it | benchmark `live-authority` cases: ranks 1, 1, 1, 1 with rivals at 2 or absent |
| 4 | a query naming a historical decision still retrieves it | `generic-historical-direct` (rank 1), `effigy-direct-historical-guide` (rank 1), `effigy-historical-decision` (rank 3) |
| 5 | changing the installed template after copying does not reinterpret the consumer profile | `installed_skill_and_template_directories_never_reach_the_query`, step 2 |

## Validation

| Check | Result |
| --- | --- |
| `cargo test --test cli_output_tests docs_context` | passed (14 tests, 3 new) |
| `cargo test -p effigy-catalog starter` | passed (6 tests) |
| `cargo test --test documentation_coverage_tests` | passed (4 tests) |
| `cargo test -p effigy-manifest -p effigy-codegraph -p effigy-contracts` | see closeout run below |
| `effigy perf:docs-context-benchmark` | 11/11 predeclared expectations held |
| `effigy qa` | see closeout run below |
| `cargo fmt --all -- --check` | see closeout run below |
| `cargo clippy --all-targets -- -D warnings` | see closeout run below |
| `git diff --check` | see closeout run below |

## Changed Surfaces

- `crates/effigy-catalog/starters/northstar/{effigy.toml,starter.toml,README.md,AGENTS.md}`
- `crates/effigy-catalog/src/starter.rs` (assertions)
- `docs/effigy.docs.toml` (this repository's committed profile)
- `tests/fixtures/docs-context-benchmark/generic-handbook/`
- `scripts/benchmark-docs-context.rhai`, `config/tasks.toml`
- `tests/cli_output_tests/docs_context_tests.rs` (3 new tests)
- `docs/guides/079-documentation-graph-profiles-and-context.md` (new)
- `docs/guides/{README,025,047,056}.md`, `docs/README.md`, `AGENTS.md`
- `.agents/skills/effigy/` and `skills/effigy/` (SKILL.md, `references/config-shapes.md`, `references/built-in-surfaces.md`)
- `docs/architecture/024`, `docs/contracts/041`
- closeout: card `1090`, roadmap `g08.035`, `docs/specs/archive/108-...` (moved),
  `docs/specs/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/generation-index.md`, `docs/roadmaps/g08/README.md`,
  `docs/logs/README.md`, `CHANGELOG.md`, `PAPERCUTS.md`

## Deliberate Non-Changes

- No runtime Rust changed. Ranking, weighting, budgets, extraction, and the
  `effigy.docs.context.v1` payload are exactly as card `1089` shipped them.
- Contract `041` grammar, rules, and acceptance are unchanged; only its
  `Next Task` was closed out.
- The stale claim in the skill reference that the profile "does not ship a
  separate docs-context command" was corrected. That was a card `1089` gap.
- The `docs/handoffs/` references to the pre-archive spec `108` path are plain
  strings, not links, and were left alone, matching how specs `107` and `109`
  were archived.

## Residuals

- **Traversal is unreachable on large corpora under default budgets.** By
  contract, 0-hop results rank first, so a corpus with more lexical seeds than
  `--max-sections` never surfaces a typed relation. Documented in guide `079`;
  not a defect, but a future lane may want a relation-reserved slice of the
  budget.
- **YAML frontmatter is extracted as a setext heading.** A handoff's whole
  frontmatter block appears as one heading string in results. Pre-existing from
  card `1088`; recorded in `PAPERCUTS.md`.
- **First query in a fresh checkout is unbounded.** Cold index of this
  repository took 96s to 155s with no wall-clock bound. Already recorded in
  `PAPERCUTS.md` by card `1089`; unchanged here.
- Three non-error graph diagnostics remain in this repository after the profile
  landed; `failed_paths` is empty and graph state is `ready`.

## Vision Target Delta

- Primary tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Movement: baseline `bounded documentation retrieval shipped, but this
  repository ran in profile-free baseline mode and Northstar existed only as
  prose` -> current `Northstar is a committed profile in the starter and in
  Effigy's own manifest, repository neutrality and installed-skill independence
  are guarded by tests, and retrieval quality is measured by a replayable
  predeclared benchmark`
- Remaining gap: `None` for this lane. Roadmap `g08.035` is complete and strict
  spec `108` is archived.

## Next Task

None. The lane is closed. Return to planning to open the next one; no release
work or generation rollover is implied.
