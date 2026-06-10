# 081 Validate Demo Browser On Real Project Cohort

Status: archived
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Trial the shipped demo browser and live terminal flow on at least two real
consumer projects before release so release readiness is based on real demo
proof, not only repo-local fixtures.

## In Scope

- select at least two real Effigy consumer repos with actual demo definitions
- validate the demo browser end to end on each repo:
  - discovery and list state
  - inspect/history visibility
  - browser-launched run/rerun
  - live terminal rendering, color, input, and stop behavior where applicable
  - retained receipt/history follow-through after runs end
- capture any repo-specific gaps that still block honest release confidence
- record the cohort, commands, outcomes, and any follow-up bugs in a log

## Out Of Scope

- new browser chrome polish unless a real-project validation bug forces it
- widening into multi-process browser controls
- release execution itself
- speculative project outreach beyond the bounded cohort

## Acceptance Criteria

- the validation log makes clear which real-project flows were exercised, which
  passed, which failed, and why
- any reduction from the ideal two-repo cohort is made explicit rather than
  implied
- any blocking gaps are converted into explicit follow-up cards instead of
  hidden in prose
- the lane leaves one explicit next release-readiness decision card

## Validation

- `cargo run --bin effigy -- qa:docs`
- repo-specific demo browser trial commands captured in the validation log
- `git diff --check`

## Stop Conditions

- no real consumer repo with demos is currently available to validate
- validation reveals a blocking terminal/browser regression that demands a new
  implementation recovery batch first
- the batch turns into general release orchestration instead of bounded demo
  cohort proof

## Outcome

- validation log recorded in
  [`../../logs/archive/2026-04/13-165446-demo-browser-real-project-cohort-validation.md`](../../logs/archive/2026-04/13-165446-demo-browser-real-project-cohort-validation.md)
- ready card opened:
  [`082-decide-demo-release-readiness-after-signal-proof.md`](./082-decide-demo-release-readiness-after-signal-proof.md)

## Next Task

Execute [`082-decide-demo-release-readiness-after-signal-proof.md`](./082-decide-demo-release-readiness-after-signal-proof.md)
to decide whether the shipped demo surface is honest enough for release prep
with Signal as the proving consumer or whether one more consumer validation
batch is still required first.
