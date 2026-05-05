# 029 - v0.x Release Readiness Audit and Gate Alignment

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05
Depends on: [`019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md`](./019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md), [`020-distribution-channel-proof-and-first-publish-closeout.md`](./020-distribution-channel-proof-and-first-publish-closeout.md), [`027-interactive-cli-prompt-expansion-and-guardrails.md`](./027-interactive-cli-prompt-expansion-and-guardrails.md), [`028-next-v0-x-readiness-and-roadmap-selection.md`](./028-next-v0-x-readiness-and-roadmap-selection.md)

## Problem

Effigy has continued meaningful `v0.x` work after the last release-readiness
assessment: distribution proof closed, runtime/container hardening stayed
green, and the prompt guardrail lane landed. The live `v0.x` release contract
still governs the repo, but the current readiness posture should be audited
before any future human-initiated release action.

That audit needs to be explicit and bounded. It should not cut a release, but
it should tell an operator whether the repo is cleanly aligned with the live
release contract and what remains before a release can be requested.

## Goal

Produce a current `v0.x` release-readiness audit against the live release
contract, current gates, changelog state, rollback assumptions, and install
channel guidance.

## Scope

- inspect the live `v0.x` release contract and release guides
- verify the current changelog and release-note posture for unreleased changes
- run non-destructive gate discovery or planning commands where useful
- validate that release-gate command names in docs still match live task
  routing
- identify any release-blocking doc or contract drift
- record a dated readiness log with clear pass/fail/deferred items

## Non-Goals

- preparing, tagging, or executing a release
- editing `.github/workflows/`
- changing release automation unless the audit exposes a small documentation or
  contract drift fix
- starting `v1.0` contract planning

## Exit Condition

This milestone is complete when the repo has a current readiness log and any
small gate/docs alignment fixes needed before a human can request the next
release flow.

## Next Task

No active ready card. A human can request the next release flow explicitly, or
the repo can stay in planning.

## Closeout

`g03.029` is complete. The audit found the non-destructive release status ready
for a suggested `0.4.0` release, repaired small release/distribution docs
drift, and did not initiate a release flow.
