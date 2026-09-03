# 118 Acowtancy Consumer Adoption Replay Strict Lane

Status: Active
Owner: Effigy orchestrator
Created: 2026-09-03
Roadmap: [`g09.003`](../roadmaps/g09/003-acowtancy-consumer-adoption-replay.md)
Decision: [`D-2026-05`](../vision/decisions/D-2026-05-consumer-adoption-cohort-replay.md)
Consumer contract: [`guide 056`](../guides/056-northstar-effigy-consumer-repo-contract.md)
Ready card: [`1111`](../roadmaps/g09/batch-cards/1111-acowtancy-consumer-adoption-replay.md)

## Outcome

Replay the current Northstar + Effigy consumer contract against a frozen real
Acowtancy revision, publish the first populated comparison scorecard, and
repair only evidence-backed drift owned by Effigy's starter or guidance.

## Fixed Decisions

- Acowtancy is the first Theme 3 pilot.
- Freeze consumer evidence at Acowtancy
  `91228893cbc2c6440b115b5aa1ee2fe34064f35b`.
- Acowtancy remains read-only. Its active cards, manifests, docs, code, runtime,
  secrets, generated state, and workarounds are outside this lane's write scope.
- Use Acowtancy's existing root discovery, doctor explain, test-plan, and
  `docs/qa:docs` / `docs/qa:northstar` authority routes. Full `effigy doctor`
  executes a repo-owned health task and is not part of the clean read-only
  replay. Record that health evidence is unavailable under this pilot boundary
  rather than starting or mutating runtime-adjacent state. Do not scaffold a
  second docs spine or substitute Effigy-owned expectations for repository
  policy.
- Record observed results and classify every mismatch as Effigy-owned,
  Acowtancy-owned, environmental, or intentionally repo-specific.
- An Effigy change is permitted only when the frozen replay proves current
  starter or guide `056` guidance is wrong or incomplete and the repair remains
  generic. No speculative product change follows from the replay.
- The first scorecard compares Effigy and Acowtancy using the same evidence
  window and marks unknown dimensions as unknown rather than inventing scores.
- Release execution, S3/provider extraction, Acowtancy product work, and cohort
  expansion are separate decisions.

## Dependency Runway

```text
governance cycle two + D-2026-05
  -> 1111 frozen Acowtancy replay and scorecard
  -> exact-head review and merge
  -> decide whether cohort expansion or a bounded Effigy-owned repair is next
```

One worker owns card `1111`. The pilot is evidence-heavy but ordinary:
acceptance and the review oracle bound the judgment, so use an economical
non-frontier day-to-day worker. Material review remains with the orchestrator.

## Whole-Lane Review Oracle

Reject the lane if any counterexample survives:

1. Evidence comes from an Acowtancy revision other than the frozen SHA, or the
   SHA is not recorded with the Effigy revision and binary identity.
2. The worker changes any Acowtancy file, index, branch, runtime, dependency,
   secret, generated artifact, or workaround.
3. A repo-owned task is replaced with an Effigy-local approximation, or a
   failure is attributed to Effigy without selector/root evidence.
4. A score is assigned without a linked command, file, or log fact, or an
   unknown dimension is silently treated as passing.
5. One pilot is presented as universal consumer compatibility evidence.
6. An Effigy edit widens beyond generic starter/guide reconciliation directly
   proved by the frozen replay.
7. Root discovery, doctor-explain routing, test-plan, docs QA, and Northstar QA
   results are not all recorded with exit, ownership, and remediation state, or
   the unavailable full-health result is silently scored as passing.
8. Acowtancy's retained workaround is removed or declared unnecessary without
   downstream revalidation owned by Acowtancy.

Smallest counterexample set: wrong consumer SHA; dirty consumer before/after;
one selector resolved to the wrong catalog; one failing gate with no ownership
classification; one unsupported numeric score; one attempted consumer edit;
one full-doctor rerun despite the proved execution boundary; and one generic
guidance edit with no frozen-replay evidence.

## Validation And Evidence

Card `1111` maps every oracle row to named proof. Run the replay without full
doctor, containers, or managed tasks. Capture machine-readable command output
where available, the pre/post Acowtancy tree state, selector ownership, the
full-doctor stop-boundary observation, and the scorecard evidence links.
Validate any Effigy docs change with `effigy qa:docs`; run broader Effigy
validation only when the actual diff warrants it.

## Stop Conditions

Stop and return to the orchestrator if the replay needs an Acowtancy edit,
runtime startup, secrets, dependency hydration, destructive cleanup, a workflow
or release mutation, an unplanned Effigy behavior change, a second consumer,
or a new universal contract claim. A consumer-owned failure is evidence, not
permission to fix that repository.

## Next Task

Card `1111` executed against frozen `91228893…`; the exact-head PR awaits
orchestrator review and merge.
