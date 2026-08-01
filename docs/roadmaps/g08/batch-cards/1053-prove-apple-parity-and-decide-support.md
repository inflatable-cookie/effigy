# 1053 - Prove Apple Parity And Decide Support

Roadmap: [`../018-apple-containers-native-backend-prototype.md`](../018-apple-containers-native-backend-prototype.md)
Strict lane: [`../../../specs/099-apple-containers-native-backend-prototype.md`](../../../specs/099-apple-containers-native-backend-prototype.md)

Status: Complete
Owner: Platform / compatibility evidence
Created: 2026-08-01
Completed: 2026-08-01

## Purpose

Measure the native backend against Effigy's real runtime contract and make an
evidence-backed support decision.

## Work

- prove gateway, VPN/network churn, named-volume, SSH-agent, secret delivery,
  Rosetta, and interrupted recovery behavior
- compare cold/warm startup, idle memory, disk, and I/O for the same stack on
  Apple Containers, Docker, and Colima
- document unsupported catalog features and diagnostics
- promote to explicit experimental support, pause for upstream gaps, or reject

## Guardrails

- do not start until `1052` is complete
- do not tune the comparison to hide per-container VM cost
- support claims must match recorded evidence

## Acceptance

- compatibility and resource matrix is reproducible
- every Memo 017 adoption gate has a pass, fail, or explicit upstream blocker
- contracts, guide, changelog, roadmap, spec, and evidence log agree on the
  final support state

## Validation

- live backend matrix
- focused runtime/gateway/data tests
- `effigy qa`
- `git diff --check`

## Stop Conditions

- pause or reject if a required product guarantee cannot be met safely

## Evidence

- [`01-144647-apple-containers-parity-decision.md`](../../../logs/2026-08/01-144647-apple-containers-parity-decision.md)

## Next Task

Batch complete. Keep Apple Containers watch-only until the reassessment gate in
guide `077` is met.
