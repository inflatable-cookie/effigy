# Demo Runner Foundation Implementation Slice Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.6`

## Summary

Locked the first implementation slice for Effigy's demo runner.

The first slice is intentionally narrow:

- registry loading from `[demos.<id>]`
- `effigy demo list`
- `effigy demo inspect`
- normalized latest-attempt inspection state

## Why This Slice Comes First

- it turns the demo registry into real product surface
- it proves the inspection contract the future TUI depends on
- it forces receipt/artifact normalization before execution logic hides weak
  data boundaries
- it avoids dragging Signal's flat script runner debt into the first batch

## Explicit Deferrals

- no `demo run`
- no `demo stop`
- no `demo rerun`
- no TUI/browser implementation
- no consumer migration work

## Follow-On

The next execution batch should implement the registry and inspection
foundation, then leave run/attempt creation for the following card.
