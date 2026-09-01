# 1098 - Isolate No-Match Benchmark from the Live Corpus

Roadmap: [`../043-docs-context-no-match-benchmark-isolation-papercut.md`](../043-docs-context-no-match-benchmark-isolation-papercut.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md), [`../../../contracts/041-documentation-graph-profile-contract.md`](../../../contracts/041-documentation-graph-profile-contract.md)
Papercut: [`PAPERCUTS.md`](../../../../PAPERCUTS.md)

Status: Ready
Owner: documentation-context benchmark matrix
Created: 2026-09-01
Ready since: 2026-09-01 papercut triage on current `main`

## Purpose

Keep the no-match regression proof durable when Effigy's own documentation
describes the benchmark and its failure mode.

## Observed Failure

The live `effigy-no-match` case succeeds only while every query term is absent
from `docs/`, `README.md`, `AGENTS.md`, `CHANGELOG.md`, and `PAPERCUTS.md`.
Writing those terms into any of those profile roots makes the report non-empty
and turns the benchmark red without a retrieval regression.

## Work

- move empty-result proof out of the live Effigy target and keep it on a
  fixture-owned corpus
- reject future empty-result benchmark cases whose target is the live repo
- preserve the remaining live authority and historical case definitions
- reconcile current matrix commentary and counts without rewriting historical
  evidence
- close the selected papercut and write one compact evidence log

## Acceptance

- [ ] no current empty-result case targets the live Effigy repository
- [ ] a future live-target empty-result case fails at matrix validation rather
      than depending on corpus vocabulary
- [ ] the fixture no-match proof remains non-vacuous and green
- [ ] remaining live case queries, expected paths, rivals, rank bounds, and
      dimensions are unchanged
- [ ] no runtime, profile, graph, ranking, traversal, budget, CLI, or JSON
      behavior changes
- [ ] `perf:docs-context-benchmark`, `effigy qa`, docs QA, and diff checks pass
- [ ] papercut, roadmap, card, evidence, and active next-task pointers close
      honestly and return to publication planning

## Review Oracle

Falsify these counterexamples before PR creation:

1. A case with `expect: "empty"` and the live repo as its target reaches query
   execution instead of failing matrix validation.
2. Removing the live empty case also removes the only fixture-owned empty proof
   or makes that proof depend on Effigy's live profile roots.
3. An unrelated live authority or historical case changes query, expected
   source, rival, rank bound, dimension, or pass criterion.
4. The benchmark reports an old case total or describes the current matrix as
   the earlier freeze state.
5. The change adds a product exclusion option or touches retrieval runtime,
   profile grammar, graph storage, refresh, CLI, or JSON behavior.

## Validation

- focused benchmark matrix-guard proof
- `effigy perf:docs-context-benchmark`
- `effigy qa:docs`
- `effigy qa`
- `git diff --check`

## Evidence Requirement

Write one dated closeout log mapping every oracle row to exact proof. Record the
fixture-only empty-case inventory, unchanged live cases, benchmark result,
papercut closure, and return to official publication planning.

## Stop Conditions

Stop if the repair requires a product query-exclusion feature, changes an
existing live authority/historical acceptance rule, changes retrieval behavior,
or expands into ranking, timeout, publication, workflow, S3, or rollover work.

## Next Task

Execute this card, then return to official catalog-pack publication planning
under contract `043`.
