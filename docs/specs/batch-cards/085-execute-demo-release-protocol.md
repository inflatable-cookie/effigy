# 085 Execute Demo Release Protocol

Status: ready
Updated: 2026-04-13
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Execute the Effigy release protocol for the shipped demo surface once the
working tree is clean and a human explicitly requests release execution.

## Preconditions

- explicit human instruction to execute the release
- clean working tree
- no unrelated local changes mixed into the release path
- normal release gates still green at execution time

## In Scope

- run the release protocol in order:
  - `effigy release status --check-gates`
  - `effigy release prepare --plan`
  - `effigy release prepare --yes --check-gates`
  - `effigy release execute --plan`
  - `effigy release execute --yes`
  - `effigy release verify-install --tag <TAG>`
- record the release outcome and any drift from the readiness checkpoint

## Out Of Scope

- workflow edits
- skipping release gates
- retagging failed releases
- more feature work unless release gates surface a true blocker

## Acceptance Criteria

- release protocol is executed only after explicit human approval
- release outcome is logged honestly
- any release blocker becomes an explicit fix batch rather than an ad hoc
  override

## Validation

- `cargo run --bin effigy -- release status --check-gates`
- `cargo run --bin effigy -- release simulate`
- `git diff --check`

## Stop Conditions

- working tree is not clean
- explicit human release instruction has not been given
- any release gate fails

## Next Task

Execute this card only after the working tree is clean and a human explicitly
asks to proceed with release execution.
