# 809 - Close g06 Proof And Residual Risk

Roadmap: [`../001-codebase-lean-down-suite.md`](../001-codebase-lean-down-suite.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Close the first lean-down tranche with before/after proof, accepted residual
risk, and explicit next targets.

## Scope

- rerun baseline size, duplication, and god-file measurements
- summarize what was actually deleted or re-owned
- record retained high-cost debt explicitly

## Acceptance

- before/after measurements are recorded
- remaining large modules and duplicate clusters are named
- front doors point at the next active queue or closeout state

## Outcome

- Rust LOC moved from `233,544` to `234,272`
- broader source/config surface moved from `236,893` to `237,622`
- god-file findings moved from `2` to `0`
- duplicate-block findings moved from `96` to `93`
- high duplicate-block findings moved from `8` to `4`
- the net line count stayed roughly flat, but ownership and oversized-file risk
  improved materially

## Residual Risk

- remaining high duplicate blocks are concentrated in:
  - CLI help topic descriptor arrays
  - one container temp-repo helper pair
- `crates/effigy-release/src/lib.rs` remains large at `1314` lines, but is now
  below the god-file warning threshold
- `src/runner/state_command.rs` remains large at `1605` total lines, but its
  code surface is no longer above the god-file threshold and more of its
  durable behavior now lives under `effigy-state`

## Suggested Validation

```bash
cargo run --bin effigy -- scan god-files --json
cargo run --bin effigy -- scan duplicate-blocks --json
cargo run --bin effigy -- docs check paths docs/roadmaps docs/specs docs/logs
```

## Next Task

None. `g06.001` is closed.
