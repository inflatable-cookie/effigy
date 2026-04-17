# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards; archive stale cards
  once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of every historical card

## Current Live Chain

- [`245-implement-routing-error-boundary-and-catalog-loading-consolidation.md`](./245-implement-routing-error-boundary-and-catalog-loading-consolidation.md)
  is the current ready card for `g02.010`.
- [`246-implement-effigy-routing-extraction.md`](./246-implement-effigy-routing-extraction.md)
  is queued behind `245`.
- [`115-implement-effigy-distribution-release-closure.md`](./115-implement-effigy-distribution-release-closure.md)
  is complete. Release execution remains deferred until the `g02.010` thread
  closes cleanly.

## Archive Rule

- closed or paused lane cards should move to `../archive/batch-cards/` once the
  lane no longer needs them in the active tree
- the active tree should stay focused on the live strict lanes rather than the
  full historical corpus
- use the governing spec plus roadmap to resolve the current ready card; this
  README is only the front door

## Next Task

Execute
[`245-implement-routing-error-boundary-and-catalog-loading-consolidation.md`](./245-implement-routing-error-boundary-and-catalog-loading-consolidation.md),
then move to
[`246-implement-effigy-routing-extraction.md`](./246-implement-effigy-routing-extraction.md).
