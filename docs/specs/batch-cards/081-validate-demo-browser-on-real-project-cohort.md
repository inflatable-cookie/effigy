# 081 Validate Demo Browser On Real Project Cohort

Status: ready
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

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

- at least two real consumer repos are exercised with the shipped demo browser
  surface
- the validation log makes clear which flows passed, which failed, and why
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

## Next Task

Execute this real-project validation batch, then leave one explicit
release-readiness boundary card instead of free-continuing into release.
