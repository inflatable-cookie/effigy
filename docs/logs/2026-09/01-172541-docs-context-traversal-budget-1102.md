# Docs Context Traversal-Budget 1102 Closeout

Status: complete
Created: 2026-09-01
Roadmap: g08.047
Batch: 1102-reserve-docs-context-traversal-slot
Handoff: `20260901-175829-docs-context-traversal-budget-1102.md`

## Summary

- `docs context` still ranks in one pass. When `max-sections >= 2` and a
  traversed candidate exists, the last section slot is held for the
  highest-ranked whole traversed section that fits the remaining byte budget.
- A saturated recurrence fixture with more 0-hop lexical hits than the section
  budget now returns the best lexical result first and one `hops > 0` result.
- One-slot queries stay on the best lexical result. No traversal still fills
  every slot from existing direct rank. An oversized traversed section is
  omitted whole with a byte-budget reason; that slot falls back to the next
  lexical result.

## Review oracle → proof

1. Lexical saturation still consumes every section slot — falsified by
   `lexical_saturation_reserves_one_whole_traversal_slot`: five `recurrence`
   lexical seeds, `max-sections = 3`, rank 3 is `handbook/playbooks/follow-up.md`
   at `hops = 1`.
2. Traversal reservation displaces or ranks ahead of the best lexical result —
   falsified by the same test: rank 1 is `handbook/playbooks/alpha.md`, `hops = 0`,
   matching unbounded rank 1. Rank 2 keeps the next lexical seed.
3. A one-section query returns traversal instead of the best direct match —
   falsified by `one_section_budget_keeps_the_best_lexical_result`: one result,
   `hops = 0`, identical to unbounded rank 1.
4. With no traversal, a reserved hole reduces the number of direct results —
   falsified by `without_traversal_every_slot_keeps_direct_rank_order`: query
   `grommet` on three relation-free docs, `max-sections = 2` returns two
   `hops = 0` results in unbounded order.
5. An oversized traversed section is sliced, exceeds bytes, or hides truncation —
   falsified by `oversized_traversed_section_is_omitted_whole`: padded follow-up
   is omitted with a named byte-budget reason, both returned sources equal their
   spans, and `used_bytes <= max_bytes`.
6. The fix adds a second query mode/ranker or weakens relevance and provenance —
   falsified by one `select` path (no new public flag, schema, or ranker);
   reserved result keeps `match_kind = relation`, `seed_path`, and
   `see-also` provenance; hop-2 `next.md` stays out at `max-hops = 1`; charter
   stays out; existing ranking tests and the 11-case benchmark still pass.

## Saturated fixture

Query `recurrence` against a separate handbook corpus:

- lexical seeds: `alpha.md` (heading match, `see-also` to follow-up), plus
  `bravo.md`, `charlie.md`, `delta.md`, `echo.md` (body matches)
- traversed target: `follow-up.md` (no `recurrence`; padded schedule text;
  `see-also` to `next.md`)
- hop-2: `next.md`
- no-traversal control: `grommet-one.md` / `grommet-two.md` / `grommet-three.md`
- unrelated authority: `handbook/reference/charter.md`

`max-sections = 3`, `max-hops = 1` ordering: `alpha.md` (lexical), next lexical,
`follow-up.md` (relation, rank 3). Hop budget reports exhaustion at 1 hop.

## Unchanged benchmark cases

`perf:docs-context-benchmark` 11/11. Frozen cases kept queries, sources, rivals,
and rank bounds. Observed ranks this run:

- fixture: charter 1, current-over-retired 1, historical-direct 1,
  relation-follow-up 4 (`rebalance.md`)
- live: contract 1, architecture 1, direct-historical-guide 1, next-task 1,
  historical-decision 2

## Changes

- `crates/effigy-codegraph/src/docs_context/mod.rs`: last-slot reservation in
  `select` for the best whole traversed candidate
- `crates/effigy-codegraph/src/docs_context/tests.rs`: saturated recurrence
  fixture and one-slot / no-traversal / oversized proofs
- this log, card `1102`, roadmap `g08.047`

Shared PAPERCUTS, changelog, contract `041`, and guide `079` stay with the
orchestrator.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`
- Movement: lexical saturation hid typed-relation evidence under `max-sections`
  → one reserved whole traversal slot when at least two slots exist
- Remaining gap: orchestrator serial merge, shared-surface integration, and
  exact-head review. Cards `1100` and `1101` stay separate.

## Validation Performed

- `cargo test -p effigy-codegraph --lib docs_context` — 24 passed
- `cargo test --test cli_output_tests docs_context` — 16 passed (public output
  unchanged; run as graph-affected direct target)
- `./target/debug/effigy graph affected --stdin` after `graph index` — changed
  paths `docs_context/mod.rs` and `docs_context/tests.rs`; exact test target
  `docs_context/tests.rs`
- `./target/debug/effigy perf:docs-context-benchmark` — 11/11
- `./target/debug/effigy qa` — 3647 passed, 1 skipped; docs and JSON-contract
  checks passed
- `cargo fmt --all -- --check` — passed
- `cargo clippy --all-targets -- -D warnings` — passed
  (`proc-macro-error2` future-incompat notice only)
- `git diff --check` — passed

## Risks

- Live corpora where 0-hop hits already fill `max-sections` now drop the last
  lexical result to keep one whole traversed section. Top-rank live-authority
  cases in the frozen matrix were unchanged; a caller that depended on lexical
  rank 8 of 8 can see a relation result there instead.

## Next Task

- Return the exact-head PR to the Effigy orchestrator. Do not merge.
