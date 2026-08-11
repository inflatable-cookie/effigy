# g08.030 - Pre-Release CI Proof

Status: Complete
Depends on: `g08.029`
Contracts: [`001`](../../contracts/001-working-rules.md),
[`039`](../../contracts/039-pre-release-ci-proof-contract.md)
Spec: [`103`](../../specs/archive/103-pre-release-ci-proof.md)

## Goal

Make a successful hosted CI run for the exact candidate source commit a hard
precondition for Effigy's release preparation and execution protocol.

## Vision Alignment

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`, `AGENT`
- Target envelope: a tag can only descend from source proven by the full hosted
  CI board.
- Vision target delta: release readiness now includes exact-commit CI evidence,
  not only local gates and post-tag publication checks.

## Execution Plan

- [x] card 1077: add exact-SHA proof, wire the release gate, align active
      protocol surfaces, and close the lane

## Non-Goals

- no automatic release or publication dispatch
- no generic GitHub dependency in the Effigy release engine
- no change to CI or binary-release workflow YAML
- no retry, bypass, or acceptance of a different green commit

## Acceptance Criteria

- [x] the self-hosted release gate rejects absent or mismatched CI evidence
- [x] the checker accepts only successful manual `ci.yml` evidence for `HEAD`
- [x] agent and human protocols dispatch and watch CI before release commands
- [x] the exact-SHA rule is durable under contract `039`
- [x] focused tests and docs validation pass

## Evidence

- [`11-182709-pre-release-ci-proof-closeout.md`](../../logs/2026-08/11-182709-pre-release-ci-proof-closeout.md)

## Next Task

Lane complete. Use the revised protocol for the next release; no release action
is implied.
