# 034 - Next v0.x Readiness and Roadmap Selection Strict Lane

Roadmap: [`g03.028`](../roadmaps/g03/028-next-v0-x-readiness-and-roadmap-selection.md)

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Choose the next live Effigy roadmap after the interactive prompt lane closed.

This lane exists to prevent strict continuation from drifting into an
unselected implementation surface. It should be short: inspect the current
authority surfaces, promote one concrete next lane if warranted, and close.

## Hard Boundaries

- do not run release prepare, execute, or tagging commands
- do not edit `.github/workflows/`
- do not reopen completed `g03.019`, `g03.020`, or `g03.027` work
- do not create multiple active ready cards
- keep the output to one promoted next task or a documented planning stop

## Evidence Anchors

- [`../roadmaps/g03/019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md`](../roadmaps/g03/019-v0.3.x-release-foundation-and-v1.0-readiness-assessment.md)
- [`../roadmaps/g03/020-distribution-channel-proof-and-first-publish-closeout.md`](../roadmaps/g03/020-distribution-channel-proof-and-first-publish-closeout.md)
- [`../roadmaps/g03/027-interactive-cli-prompt-expansion-and-guardrails.md`](../roadmaps/g03/027-interactive-cli-prompt-expansion-and-guardrails.md)
- [`../roadmaps/backlog/README.md`](../roadmaps/backlog/README.md)
- [`../contracts/README.md`](../contracts/README.md)

## Current Ready Card

[`372-decide-next-live-roadmap-after-prompt-lane-closeout.md`](./batch-cards/372-decide-next-live-roadmap-after-prompt-lane-closeout.md)

## Exit Condition

This lane closes when card `372` selects the next live roadmap target or
records why no ready implementation card should be opened.

## Next Task

Execute [`372-decide-next-live-roadmap-after-prompt-lane-closeout.md`](./batch-cards/372-decide-next-live-roadmap-after-prompt-lane-closeout.md).
