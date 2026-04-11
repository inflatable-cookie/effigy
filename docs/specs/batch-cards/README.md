# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning

## Active Batch Cards

- [`003-decide-override-conflict-and-explainability.md`](./003-decide-override-conflict-and-explainability.md)

## Next Task

Execute the active ready card if it is still honest; otherwise return the lane
to planning and refresh the currentness surfaces.
