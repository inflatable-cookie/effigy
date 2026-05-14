# State Command Config Owner Extraction

Date: 2026-05-14
Roadmap: `g06.002`
Card: `802`

## Summary

Moved the composed `[state]` config model and stack-selection logic out of the
runner and into `effigy-state`.

This is the first real `g06` code-reduction slice. The runner still owns CLI
adaptation, manifest resolution, execution, and rendering, but it no longer
owns the state config schema and conversion layer.

## What Changed

- added `crates/effigy-state/src/config.rs`
- moved into `effigy-state`:
  - composed `[state]` config model
  - stack selection for manifest/apply paths
  - capture profile lookup
  - compact inline task normalization for state hook/task definitions
- rewired `src/runner/state_command.rs` to use the shared owner
- updated state command tests to use the new owner boundary

## Size Delta

- `src/runner/state_command.rs`
  - baseline: `2150` lines
  - current: `1918` lines
  - delta: `-232`

New owned code added:

- `crates/effigy-state/src/config.rs`: `308` lines

This slice is still a win because the config schema now has one durable owner
instead of living runner-private inside a command module.

## Retained Runner Ownership

Still intentionally runner-owned after this slice:

- command argument adaptation
- composed-manifest loading and standalone manifest resolution
- apply/capture execution orchestration
- report writing and final text rendering
- CLI-facing capture request validation

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`
- moved in this report: state config schema and state stack selection moved
  from runner-private ownership into `effigy-state`; `state_command.rs` shrank
  by `232` lines
- remains open:
  - further `state_command.rs` trimming around apply/capture orchestration
  - `effigy-release/src/lib.rs` split
  - fixture and duplicate-block convergence

## Validation

Commands used:

```bash
cargo fmt --all
cargo test -p effigy-state
cargo test state_command
```
