# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning

## Active Batch Cards

- [`007-decide-demo-model-boundaries-and-registry-shape.md`](./007-decide-demo-model-boundaries-and-registry-shape.md) (complete)
- [`008-decide-demo-runner-lifecycle-and-artifact-boundaries.md`](./008-decide-demo-runner-lifecycle-and-artifact-boundaries.md) (complete)
- [`009-decide-demo-coverage-and-gap-model.md`](./009-decide-demo-coverage-and-gap-model.md) (complete)
- [`010-decide-demo-browser-and-tui-contract.md`](./010-decide-demo-browser-and-tui-contract.md) (complete)
- [`011-reconcile-demo-contract-against-signal-pilot.md`](./011-reconcile-demo-contract-against-signal-pilot.md) (complete)
- [`012-decide-demo-runner-foundation-implementation-slice.md`](./012-decide-demo-runner-foundation-implementation-slice.md) (complete)
- [`013-implement-demo-registry-and-inspection-foundation.md`](./013-implement-demo-registry-and-inspection-foundation.md) (complete)
- [`014-implement-demo-run-and-attempt-foundation.md`](./014-implement-demo-run-and-attempt-foundation.md) (complete)
- [`015-decide-demo-active-attempt-stop-and-rerun-contract.md`](./015-decide-demo-active-attempt-stop-and-rerun-contract.md)

## Next Task

Execute the active ready card if it is still honest; otherwise return the lane
to planning and refresh the currentness surfaces.
