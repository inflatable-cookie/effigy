# g09.007 Docs Context Exact Identifier Retrieval

Status: Ready
Created: 2026-09-05
Spec: [`121`](../../specs/121-docs-context-exact-identifier-retrieval-strict-lane.md)
Card: [`1114`](./batch-cards/1114-docs-context-exact-identifier-retrieval.md)
Contract: [`041`](../../contracts/041-documentation-graph-profile-contract.md)
Guide: [`079`](../../guides/079-documentation-graph-profiles-and-context.md)
Origin: card `1113` closeout evidence
([`05-113123`](../../logs/2026-09/05-113123-docs-context-latency-and-freshness-1113.md)),
Chatterbox ruling 2026-09-05; operator confirmed 2026-09-05

## Purpose

A query that names an exact identifier finds the section that contains it.

## Problem

`effigy docs context "catalog_tasks"` does not return guide `026`, which
contains the literal `catalog_tasks`, in the top 32. The query splitter in
`docs_context/rank.rs` breaks on every non-alphanumeric character, so the
identifier becomes the common words `catalog` and `tasks`; the phrase bonus
applies only to queries containing whitespace; and the shared FTS index
tokenises the same way. Documents dense in those two words outrank the one
exact match. Contract `041` retrieval rule 2 promises seeding from exact
source-text matches; for identifiers it does not hold. Agents query
identifiers constantly (pilot case K4 was an observed cross-project repeat),
and `g09.006` cannot claim recall while this stands.

## Decision

- An identifier-shaped query term (contains `_`, `-`, `.`, `::`, or `/`
  between alphanumerics) is kept whole as an exact term in addition to its
  split words. Exact whole-term containment of the identifier in a section,
  heading, path, or field outranks split-word density from other documents.
- Candidate recall may use the split words or an FTS phrase; ranking must
  use the exact identifier with whole-term boundaries (`contains_term`).
- The benchmark matrix gains one frozen exact-identifier case against
  Effigy and one against the generic fixture (a freeze, recorded in the
  script's history), so this cannot regress silently.
- No change to budgets, freshness, locking, traversal, currentness, or
  authority rules. Latency budgets from spec `120` must still hold.

## Cards

- [ ] [`1114`](./batch-cards/1114-docs-context-exact-identifier-retrieval.md) — ready

## Acceptance

- `docs context "catalog_tasks" --max-sections 3` returns guide `026`'s
  containing section in the top 3 with a match reason naming the exact term
- a fixture identifier case behaves the same; the two new benchmark cases
  and all eleven existing cases pass
- a non-identifier query's ranking is unchanged across the existing matrix
- warm query still succeeds under `EFFIGY_GRAPH_TIMEOUT_MS=5000` on Effigy
- `graph` still does not match `graphql`; `catalog_tasks` does not match
  `catalog_tasks_v2` as an exact term

## Non-Goals

- stemming, fuzzy matching, embeddings, or synonym handling
- changing FTS tokenisation for code symbols or the code-graph search
- cross-repository routing, K5 rephrasing, or any change to `g09.006`
- budget, freshness, or lock changes

## Dispatch Manifest

Published for the coordinator at the promoting commit on `main`.

- **Lane:** card `1114`, roadmap `g09.007`, strict spec `121`. State: ready.
- **Prerequisites:** clean `main` at or after the promoting commit; no other
  active strict lane. **Completion:** PR merged with evidence log, card,
  roadmap, spec, guide, benchmark freeze, and changelog closed out.
- **Owned mutable paths:** `crates/effigy-codegraph/src/docs_context/**`,
  `crates/effigy-codegraph/src/storage.rs` (search query helpers only),
  `scripts/benchmark-docs-context.rhai` (new cases plus freeze history),
  `tests/fixtures/docs-context-benchmark/generic-handbook/**` (one identifier
  fixture document if needed), tests under those crates and `src/tests/**`,
  `docs/guides/079-documentation-graph-profiles-and-context.md`.
  **Reserved shared closeout surfaces:** `CHANGELOG.md` `[Unreleased]`,
  `docs/logs/2026-09/`, `docs/logs/README.md`, this roadmap, card `1114`,
  spec `121`, contract `041` (retrieval rule wording if a drift trigger
  fires), `docs/specs/README.md`, `docs/roadmaps/README.md`,
  `docs/roadmaps/g09/README.md`.
- **Concurrency:** no approved siblings; no serial edges. Single lane.
- **Worker capability class:** economical non-frontier day-to-day
  implementation worker.
- **Acceptance evidence and review oracle:** card `1114` acceptance and spec
  `121` whole-lane oracle; benchmark run with the new freeze, focused tests,
  warm latency check, `effigy qa`, fmt, clippy, `git diff --check`; one dated
  evidence log.
- **Stop conditions and escalation owner:** spec `121` stop conditions.
  Planning questions escalate to the coordinator, then Chatterbox.

## Next Task

Execute card `1114`. On closeout, Chatterbox resumes the `g09.006` freeze
conversation with the operator.
