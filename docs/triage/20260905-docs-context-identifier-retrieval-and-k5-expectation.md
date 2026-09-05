# Docs Context Identifier Retrieval and K5 Expectation

Status: open — re-planned out of card `1113`; unscheduled
Created: 2026-09-05
Owner: chatterbox
Source: PR `91` evidence (worker log
`docs/logs/2026-09/05-113123-docs-context-latency-and-freshness-1113.md`),
coordinator pre-PR decision request, Chatterbox ruling 2026-09-05
Contract: [`041`](../contracts/041-documentation-graph-profile-contract.md)
Feeds: [`g09.006`](../roadmaps/g09/006-cross-repository-source-routing.md)
gate decision

## Issue 1 — exact identifier queries miss their source (Effigy defect)

`effigy docs context "catalog_tasks"` does not return
`docs/guides/026-json-payload-examples.md` in the top 3 or top 32, although
the guide contains the literal `catalog_tasks` (lines ~905 and ~924). Query
tokenisation splits the snake_case identifier into the common words
`catalog` and `tasks`, so documents dense in those words outrank the one
exact match. Agents ask for identifiers constantly (K4 was an observed
cross-project repeat). This is a retrieval-rule defect under contract `041`
("seed from exact ... source-text matches"), not a latency issue, and was out
of scope for spec `120`, which forbids ranking changes.

Known: reproduced at PR `91` head `d8b9b36d`. Unknown: whether the fix is
an exact-token seed that preserves identifiers before word splitting, and
what the benchmark matrix needs (a frozen exact-identifier case) so it cannot
regress. Candidate lane: bounded contract-`041` retrieval papercut with one
new benchmark case; must not change budgets or freshness.

## Issue 2 — K5 expectation requires inference (pilot expectation defect)

The pilot expected guide `051` for "Does release execute publish a GitHub
Release?", but guide `051` never contains the phrase; it says pushing a tag
alone does not publish and that the release workflow publishes artifacts.
Answering K5 needs inference across sections, which contract `041` forbids
(`docs context` returns evidence, not an answer). K5 is a valid question
but not a valid exact-section retrieval oracle as written.

Known: guide `051` ranks 24 of 32 for the phrase. Next check: with the
Northstar Chatterbox, rephrase K5 into an evidence-shaped query (e.g. the
guide's own words about publication) or split it into a tool-behaviour
question and a consumer-obligation question before it enters the `g09.006`
replay set.

## Next Task

Reopen when `g09.006` planning starts, or earlier if the operator wants the
identifier defect fixed as a standalone papercut. Issue 1 should be fixed
before any cross-repository replay claims recall.
