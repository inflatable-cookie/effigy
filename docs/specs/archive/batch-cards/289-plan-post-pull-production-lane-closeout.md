# 289 Plan Post Pull Production Lane Closeout

Status: complete
Updated: 2026-04-18
Roadmap: `g02.015`
Spec: `docs/specs/015-persistent-data-and-volume-lifecycle-strict-lane.md`

## Objective

Choose the next bounded `g02.015` step now that generated-compose
`data.pull_production` hook ownership is real.

## Context

`280`, `282`, `284`, `286`, and `288` now cover reset retention, inventory,
transfer, media lifecycle, and bounded production-pull orchestration on the
generated-compose path.

The remaining roadmap residue is narrower:

- whether task-owned seeding needs any more product planning beyond the
  existing task, workspace binding, exec, and Rhai surfaces
- whether the lane needs one real-project proof batch before closeout
- whether `g02.015` can close on the current bounded contract

Decision:

- no separate seed-specific product batch is needed on this lane; task-owned
  seeding is already adequately carried by the shipped task, exec,
  workspace binding, and Rhai surfaces
- `g02.015` should take one bounded real-project proof batch before closeout
  so the persistent-data contract is not left consumer-unproven
- the next explicit step is `290`, a real-project proof of the generated-compose
  persistent-data loop rather than another feature widening batch

This card resolves the planning boundary. The next step is the bounded proof
batch in `290`, not another guessed feature widening move.

## In Scope

- assess whether `g02.015` needs one more proof or closeout batch
- decide whether task-owned seeding needs any further product-owned work
- update the strict-lane front door so one explicit next card exists or the
  lane closes honestly

## Out Of Scope

- execution work
- direct `compose_file` ownership widening
- broad orchestration redesign
- unrelated roadmap rollover work

## Acceptance

- one explicit next step exists for `g02.015`
- the lane does not free-continue after landed `288`
- the front-door planning surfaces stop pointing at already-landed `288`
- the decision is explicit that seeding remains task-based rather than becoming
  a new product-owned abstraction batch

## Next Task

Execute `290` to prove the generated-compose persistent-data loop in one real
project, then close `g02.015` or record only any proof-exposed residue.
