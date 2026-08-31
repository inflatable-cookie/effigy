# Bounded Documentation Context Query

Status: complete
Created: 2026-08-31
Roadmap: g08.035
Card: 1089
Spec: 108
Contract: 041
Architecture: 024
Predecessor evidence: [`30-004016-documentation-graph-1088.md`](./30-004016-documentation-graph-1088.md)

## Summary

- Added `effigy docs context <QUERY> [--max-sections N] [--max-bytes N]
  [--max-hops N]` over the shared graph. No second store, refresh path, or
  remote/model inference.
- `effigy-codegraph` owns scope, lexical seeding, typed-relation traversal,
  ranking, budgeting, and the typed report. `effigy-cli` owns grammar and help;
  the built-in docs shell owns root selection, rendering, envelope, and exit.
- Retrieval is evidence selection. Every result is an exact repository slice
  with span, provenance, extracted fields, relation path, and machine-readable
  match reasons. Nothing is summarized or generated.
- Runtime vocabulary stayed generic. Fixtures use handbook/playbook/bulletin
  vocabulary; no Northstar path, status, kind, or relation entered the query
  logic. Northstar profile adoption remains card `1090`.

## Retrieval Model

Scope is repository-owned: profile roots when `[docs_policy.graph]` is
configured, otherwise every indexed Markdown file.

1. Terms are split on non-alphanumeric boundaries and lowercased.
2. Corpus frequency is derived from the shared FTS `source` rows plus stored
   path, heading, and field facts. A term reaching more than half of a corpus of
   at least 8 scoped documents is reported with `weighted: false` and stays out
   of scoring. Below that floor a shared term is ordinary vocabulary, so the
   filter does not apply. If every term would be dropped, all terms are kept.
   This replaces a language- or repository-specific stop-word list.
3. Sections score on their own text — the section span cut at the first nested
   heading. Evidence is still the full hierarchical section, but a document's
   top-level heading cannot absorb every nested match and answer with the whole
   file.
4. Path and field evidence is document-level and lands on one section, so a path
   match cannot flood the budget.
5. Typed `doc-rel` edges expand breadth-first for at most `--max-hops`.
6. Order is `hops`, then relevance, then heading depth, then currentness, then
   authority, then path, span start, and record id. Filesystem iteration never
   reaches ranking.
7. Overlapping spans from one document deduplicate toward the higher-ranked
   (more specific) section.

Only candidates with non-zero lexical relevance, or reached over a configured
relation from such a candidate, are eligible. Authority and currentness cannot
introduce a result.

## Fixture Cases

Generic vocabulary: roots `handbook`; fields `state` / `steward`; currentness
`live` / `retired`; kinds `playbook` (80) / `bulletin` (20, default historical)
/ `charter` (100); relation `see-also`.

| Case | Result |
| --- | --- |
| Unrelated authority-100 `charter.md` vs query `widget calibrator` | absent from the report; relevance 0 is not eligible |
| Query `widget calibrator recall` | historical authority-20 `handbook/bulletins/old.md#widget-calibrator-recall` ranks 1 |
| Query `escalation rota` over identical live/retired sections | `playbooks/rotation.md` (current) precedes `bulletins/rotation-notice.md` (historical) at equal relevance |
| Query `quokka telemetry` | success, `results: []`, `truncated: false`, `used_bytes: 0`, stable profile/freshness metadata |
| Repeated `widget calibrator` | byte-identical results and terms |
| Nested `# Retired widget bulletin` vs `## Widget calibrator recall` | one result; the h2 wins and the overlapping h1 is dropped |
| Baseline repository (no `[docs_policy.graph]`) | same schema; kind `document`, authority `0`, currentness `unknown`, no traversal |
| Profile-only authority edit `20` -> `45` | fingerprint changes, result authority becomes `45`, one `.effigy/graph` directory |

## Exact Provenance

Every returned `source` is asserted equal to `content[span.start.byte..span.end.byte]`
of the named file, and `bytes` equals `source.len()`. Example from the profiled
fixture, query `widget recall`:

```text
1. handbook/bulletins/old.md#widget-recall [bulletin] authority 20 currentness historical
   lines 5-8 bytes 36-83 (47 bytes) 0 hop(s) via lexical
   match: heading contains phrase `widget recall`; ...
   fields: state=retired
```

`record_id` is `symbol:doc:handbook/bulletins/old.md:#widget-recall`;
provenance is `markdown-anchors` `0.2.0`, confidence `exact`, detail `heading`.

## Budget Proofs

Defaults are 8 sections, 24000 bytes, 1 hop. Maxima are 32, 100000, 3. Budgets
must be positive; `0` and out-of-range values are usage errors in both the
parser and the library.

| Budget | Proof |
| --- | --- |
| `--max-sections 1` | one result identical to the unbounded rank 1; `section_budget_reached: true`; `omitted_sections >= 1`; reason `section budget reached after 1 sections`; next step names `--max-sections` |
| `--max-bytes <rank-1 bytes>` | exactly the rank-1 section, byte-identical to the unbounded run; `used_bytes` equals that section; `byte_budget_reached: true`; reason `byte budget omitted ...` |
| `--max-hops 1` | one lexical seed `playbooks/setup.md`, one relation result `playbooks/ops.md` with `relation_path[0].relation = see-also`; `hop_budget_reached: true` |
| `--max-hops 2` | adds `playbooks/rotation.md` at `hops: 2`; `hop_budget_reached: false` |

A section that does not fit the byte budget is omitted whole and named in
`truncation.reasons` (capped at 8 named omissions). No partial section is ever
emitted, and skipping one oversized section does not reorder the sections that
are returned.

## Public Surface

- text: header line, profile/freshness/budget/result counts, then per result the
  path/anchor, kind, authority, currentness, line and byte span, hop count and
  match kind, match reasons, fields, relation path, and the exact section text;
  then truncation, diagnostics, and next steps.
- JSON: `effigy.docs.context.v1` inside the standard `effigy.command.v1`
  envelope, carrying query, repo root, profile state/fingerprint/roots/fields/
  kinds/relations/scoped documents, graph freshness, requested/applied/default/
  maximum budgets, weighted query terms, results, truncation, diagnostics, and
  next steps.
- An empty or whitespace-only query is a usage error and exits non-zero. No
  match is a successful empty report.

## Validation

| Check | Result |
| --- | --- |
| `cargo test -p effigy-codegraph -p effigy-cli -p effigy-contracts` | passed (101 + 10 + 5 tests; 16 new `docs_context` tests) |
| `cargo test --lib` | passed (1416 tests; 4 new `docs context` parse tests) |
| `cargo test --test cli_output_tests` | passed (268 tests, 1 ignored; 8 new `docs_context_tests`) |
| `cargo test --test documentation_coverage_tests` | passed (4 tests) |
| `effigy contracts check-json --print-selected=text` | passed; `effigy.docs.context.v1` selected and validated |
| `effigy docs check links` | passed |
| `effigy docs check json-examples` | passed |
| `effigy docs check index` | passed |
| `effigy docs check forbidden` (agent defaults) | passed |
| `cargo fmt --all -- --check` | passed |
| `cargo clippy --all-targets -- -D warnings` | passed |
| `cargo clippy -p effigy-codegraph -p effigy-cli -p effigy-contracts --all-targets -- -D warnings` | passed |
| `git diff --check` | passed |
| `effigy doctor` | `ok:19 warn:1 err:0` before and after (pre-existing god-files warning) |

## Affected Analysis

`EFFIGY_GRAPH_TIMEOUT_MS=0 effigy graph affected --stdin` over the 35 changed
paths, after the refresh, reported the changed set plus only `[heuristic]`
neighbors: `effigy-bootstrap` and `effigy-builtin` test files reached through
`contains` traversal and unresolved `call`/`import` symbol matching, and the
`qa` / `qa:architecture` task family. As in card `1088`, those heuristic
neighbors are not the real validation surface for this change, so validation ran
the direct crate, CLI parse, CLI output, documentation-coverage, and JSON
contract suites listed above instead. The command itself notes that its output
is bounded graph evidence, not exhaustive test proof.

The first graph refresh on this worktree paid a cold index (3784 files, ~204s)
because the worktree had no prior graph. That cold path is unbounded for
`docs context` and is recorded in `PAPERCUTS.md`; it was not fixed here.

## Changed Surfaces

- `crates/effigy-codegraph/src/docs_context/` (`mod.rs`, `payload.rs`,
  `rank.rs`, `scope.rs`, `tests.rs`) and `lib.rs` re-exports
- `crates/effigy-cli`: `DocsSubcommand::Context`, `docs context` parser, docs
  help topic, docs command-surface description
- `src/runner/docs_command/context.rs` and its dispatch
- `crates/effigy-contracts`: `<fixture_docs_context>` profiled fixture
- `docs/contracts/json-schema-index.json`,
  `docs/guides/025-command-reference-matrix.md`,
  `docs/guides/026-json-payload-examples.md`,
  `docs/guides/029-docs-qa-checklist-and-validation.md`, `CHANGELOG.md`
- tests: `src/tests/lib_tests_parse_tests/docs_and_contracts_option_tests.rs`,
  `tests/cli_output_tests/docs_context_tests.rs`

## Deliberate Non-Changes

- `[docs_policy.graph]` is still absent from this repository's own
  `effigy.toml`; Effigy runs `docs context` in baseline mode. Authoring the
  Northstar profile is card `1090`.
- `.agents/skills/effigy` and `skills/effigy` are unchanged. Agent adoption
  guidance for `docs context` belongs to card `1090`.
- Contract `041` was not modified. The shipped command form, budgets, defaults,
  maxima, schema id, and retrieval rules match it as written.

## Readiness Transition

Card `1089` is complete. Every acceptance item is evidenced above:
baseline/profiled parity, authority gating, historical retrievability, enforced
and reported budgets, stable ordering, text/JSON agreement, and no generated
summaries. Card `1090` is now ready: prove generic and Northstar profiles,
publish adoption guidance, benchmark retrieval, and close the lane. Card `1090`
was not implemented here.

## Vision Target Delta

- Tags: `OPERATE`, `MAINT`, `ROUTE`, `CONTRACT`
- Baseline: exact Markdown sections, typed facts, and typed relations were
  stored but could only be assembled by hand.
- Current: one bounded, deterministic, provenance-carrying retrieval command in
  text and versioned JSON, usable with or without a repository profile.
- Open: card `1090` generic plus Northstar adoption proof, benchmark, and lane
  closeout.

## Next Task

Execute ready card
[`1090`](../../roadmaps/g08/batch-cards/1090-prove-generic-and-northstar-profiles.md).
