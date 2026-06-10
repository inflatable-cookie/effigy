# 802 - Trim State Command Domain Ownership

Roadmap: [`../002-state-command-domain-split-and-shell-trim.md`](../002-state-command-domain-split-and-shell-trim.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Land the first bounded `state_command.rs` size reduction by moving the
composed state config schema and stack-selection logic into `effigy-state`.

## Scope

- extract composed `[state]` config models out of the runner
- move state stack selection/apply selection into `effigy-state`
- move state capture profile lookup into `effigy-state`
- keep CLI parsing, manifest resolution, execution, and rendering in runner

## Acceptance

- `state_command.rs` is materially smaller
- `effigy-state` owns the moved state config schema and conversion logic
- focused state tests stay green

## Completed

- Added [`crates/effigy-state/src/config.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-state/src/config.rs).
- Moved composed state config parsing, apply selection, and capture profile
  lookup into `effigy-state`.
- Rewired `src/runner/state_command.rs` to use the shared owner.
- Reduced `state_command.rs` from `2150` lines to `1918` lines.
- Logged the slice in
  [`../../../logs/archive/2026-05/14-202500-state-command-config-owner-extraction.md`](../../../logs/archive/2026-05/14-202500-state-command-config-owner-extraction.md).

## Next Task

Execute `803`.
