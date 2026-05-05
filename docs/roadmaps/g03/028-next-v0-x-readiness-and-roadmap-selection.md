# 028 - Next v0.x Readiness and Roadmap Selection

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05
Depends on: [`019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md`](./019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md), [`020-distribution-channel-proof-and-first-publish-closeout.md`](./020-distribution-channel-proof-and-first-publish-closeout.md), [`027-interactive-cli-prompt-expansion-and-guardrails.md`](./027-interactive-cli-prompt-expansion-and-guardrails.md)

## Problem

`g03.027` closed the active prompt guardrail lane, and the roadmap/spec front
doors now correctly advertise no active ready card. That is the right stop
state for strict continuation, but it leaves the next product move undefined.

The repo has several completed anchors that can influence the next lane:

- the `v0.x` release contract remains the live authority surface
- distribution proof is closed for Homebrew, GitHub Releases, and source
  install
- runtime/container hardening and prompt guardrails have landed
- no backlog item is currently promoted

Continuing by implementation would be guesswork. The next step is a bounded
selection pass that chooses the next live roadmap from the completed evidence.

## Goal

Select the next executable `g03` roadmap lane and leave the front doors pointed
at a concrete ready card.

## Scope

- audit the completed `g03.019`, `g03.020`, and `g03.027` anchors
- check whether the next move should be release readiness, `v1.0` contract
  planning, adoption rollout, documentation cleanup, or a small code lane
- promote exactly one next roadmap target, or explicitly leave the repo in
  planning if none is justified
- avoid release execution; release commands require explicit human instruction

## Non-Goals

- cutting or preparing a release
- editing release workflows
- reopening completed prompt, distribution, or runtime-hardening lanes
- creating a broad backlog without a concrete promoted next task

## Exit Condition

This milestone is complete when the next live roadmap target is selected, or
the repo has a documented reason to remain in planning with no ready card.

## Next Task

Execute [`373-audit-v0-x-release-readiness-and-gate-alignment.md`](../../specs/batch-cards/373-audit-v0-x-release-readiness-and-gate-alignment.md).

## Closeout

`g03.028` selected `g03.029` as the next live roadmap. The next lane is a
bounded `v0.x` release-readiness audit and gate-alignment pass, not a release
execution flow.
