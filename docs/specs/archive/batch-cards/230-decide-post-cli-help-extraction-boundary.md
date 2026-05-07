# 230 Decide Post CLI Help Extraction Boundary

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/cli_help.rs` shell is now honest enough to
pause after the help topic surface moved into `effigy-cli`.

## In Scope

- inspect what still remains in `src/cli_help.rs`
- decide whether the remaining shell is now mostly adapter work (HelpRenderer
  bridge, CLI header theming, error mapping)
- record the decision honestly in the lane surfaces
- set the next ready card only if another bounded CLI-help slice is still
  justified

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup
- speculative UI crate work (job #6 in `g02.017`)
- shifting to another seam without recording the CLI-help boundary first

## Acceptance Criteria

- the post-`229` CLI-help boundary is recorded clearly
- the next move is explicit:
  - either CLI-help pauses cleanly
  - or one more bounded CLI-help card is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`231-decide-next-src-shell-cleanup-priority-after-cli-help-pause-boundary.md`](./231-decide-next-src-shell-cleanup-priority-after-cli-help-pause-boundary.md)
to pick the next `/src` cleanup priority after pausing CLI help.
