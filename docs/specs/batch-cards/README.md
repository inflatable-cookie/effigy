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

- [`248-implement-runner-utility-prerequisites-for-effigy-builtin.md`](./248-implement-runner-utility-prerequisites-for-effigy-builtin.md)
  is a ready card for `g02.010` (runner-side utility relocations and
  inversions).
- [`249-implement-effigy-scan-extraction.md`](./249-implement-effigy-scan-extraction.md)
  is a ready card for `g02.010` (extract `effigy-scan`; single-card
  per the `247` decision). Independent of `248`.
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

Two ready implement cards, independent of each other:

- [`248-implement-runner-utility-prerequisites-for-effigy-builtin.md`](./248-implement-runner-utility-prerequisites-for-effigy-builtin.md)
  — relocate six runner-side utilities + two inversions. Prereq for
  the future `effigy-builtin` extraction.
- [`249-implement-effigy-scan-extraction.md`](./249-implement-effigy-scan-extraction.md)
  — extract `src/runner/scan/**` into `effigy-scan` with a
  `ScanError` boundary.

The `effigy-builtin` implement card opens once both land.
