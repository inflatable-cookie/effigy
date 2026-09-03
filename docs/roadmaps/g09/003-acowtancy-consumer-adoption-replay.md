# g09.003 Acowtancy Consumer Adoption Replay

Status: Complete
Created: 2026-09-03
Spec: [`118`](../../specs/118-acowtancy-consumer-adoption-replay-strict-lane.md)
Decision: [`D-2026-05`](../../vision/decisions/D-2026-05-consumer-adoption-cohort-replay.md)
Consumer contract: [`guide 056`](../../guides/056-northstar-effigy-consumer-repo-contract.md)

## Purpose

Turn Theme 3 into current evidence by replaying Effigy's portable Northstar
consumer contract against Acowtancy without taking ownership of Acowtancy work.

## Sequence

1. [`1111`](./batch-cards/1111-acowtancy-consumer-adoption-replay.md) —
   **Complete**: replay executed clean at frozen `91228893…`; drift
   classified, first populated comparison scorecard published, and the
   proved guide `056` gap reconciled. PR `88` merged at `9c05a883`.

The lane is serial because replay evidence, ownership classification,
scorecard interpretation, and any resulting Effigy guidance repair form one
review boundary. Acowtancy's own active lanes remain independent and untouched.

## Acceptance

- current Effigy is exercised against the frozen Acowtancy revision
- root routing plus docs and Northstar contract gates have exact recorded
  outcomes and ownership classification
- the first populated scorecard compares Effigy and Acowtancy without invented
  evidence or universal claims
- Acowtancy remains at the exact frozen HEAD with unchanged tracked files and
  Git state
- only demonstrated generic Effigy starter/guide drift may change
- evidence names whether another consumer replay is warranted

## Non-Goals

- Acowtancy implementation, cleanup, workaround removal, or planning changes
- runtime/container startup, secrets, state application, or dependency install
- Effigy release work
- S3/provider extraction
- automatic portfolio rollout or a compatibility claim from one pilot

## Next Task

Decide cohort expansion versus a second bounded repair at the next planning
checkpoint; Acowtancy-owned health and workaround revalidation remain outside
this roadmap.
