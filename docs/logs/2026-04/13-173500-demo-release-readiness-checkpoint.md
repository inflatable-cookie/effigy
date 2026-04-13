# Demo Release Readiness Checkpoint

Date: 2026-04-13
Roadmap: `g02.003`
Card: [`083-prepare-demo-release-readiness-checkpoint.md`](../../specs/batch-cards/083-prepare-demo-release-readiness-checkpoint.md)

## Summary

Prepared the demo-surface release-readiness checkpoint after Signal proved the
real consumer path and the local release-prep commands showed Effigy is ready
to prepare a `0.2.13` release once the working tree is clean and release
execution is explicitly requested.

## Vision Target Delta

- Tags: `DEMO`, `RELEASE`, `OPERATE`
- Moved: `demo surface proven but still sitting in strict implementation lane`
  -> `demo surface packaged for release review with explicit residual risks and
  operator recommendation`
- Remaining: decide whether to enter actual release-execution work or stop for
  one more bounded pre-release fix

## Shipped Demo Surface In Scope For Release

- manifest-owned demo registry with included config support
- native `[demos.*]` declarations
- inline demo `run = [ ... ]` sequences
- inspect, history, run, stop, rerun
- browser list/detail/history/artifacts/terminal views
- live terminal fidelity, color, input, and resize
- concurrent-runner projection shape/process summary/output provenance truth

## Real Consumer Proof

- Signal now acts as the proving consumer repo
- proof included:
  - native demo manifest adoption
  - included demo fragment composition
  - browser trials
  - headless and interactive demo flows
  - retained history follow-through

## Residual Risks

- only one real consumer repo was validated before release prep
- some consumer repos may still need local script/runtime cleanup after adopting
  the demo surface
- release execution itself still requires a clean working tree and explicit
  human instruction per release protocol

## Release-Prep Evidence

- `cargo run --bin effigy -- qa:docs`
  - result: pass
- `cargo run --bin effigy -- qa:ci`
  - result: pass
- `cargo run --bin effigy -- release simulate`
  - result: pass
  - suggested version: `0.2.13`
  - ready to prepare and execute: `yes`

## Recommendation

- release execution work is justified next once:
  - the working tree is clean
  - the human explicitly asks to execute the release protocol
- no further demo-surface implementation work is required before that boundary

## Outcome

Opened ready card
[`084-decide-demo-release-execution-readiness.md`](../../specs/batch-cards/084-decide-demo-release-execution-readiness.md).

## Next Task

- Execute `084-decide-demo-release-execution-readiness.md`
