# 093 - Init Setup Wizard Strict Lane

Roadmap: [`g07.050`](../roadmaps/g07/050-init-setup-wizard-suite.md)
Related planning:
- [`g07.051`](../roadmaps/g07/051-init-context-inventory-and-checklist-contract.md)
- [`g07.052`](../roadmaps/g07/052-tty-init-wizard-engine-and-prompt-flow.md)
- [`g07.053`](../roadmaps/g07/053-setup-job-adapters-and-mutation-boundaries.md)
- [`g07.054`](../roadmaps/g07/054-noninteractive-init-action-execution-and-migration-paths.md)
- [`g07.055`](../roadmaps/g07/055-init-wizard-proof-docs-and-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Expand `effigy init` from a baseline file initializer into the bounded setup
front door for humans and agents.

## Lane Posture

Posture: `closed-no-ready-card`

This lane exists because init already owns repo setup intent. The missing work
is not a second onboarding command. It is a broader, better-structured init
that can:

- prompt in a TTY
- emit a checklist for agents
- execute the same setup jobs non-interactively

## Hard Boundaries

- no second top-level onboarding command beside `effigy init`
- no generic interactive framework widened beyond init
- no hidden release, deploy, state, or distribution mutation
- no fake setup jobs unsupported by current product surfaces
- no CI/non-TTY prompt behavior

## Execution Order

1. `1000`: open the lane and currentness surfaces
2. `1001`: define the shared context inventory and checklist contract
3. `1002`: build the TTY wizard engine and prompt flow
4. `1003`: wire setup-job adapters and mutation boundaries
5. `1004`: add non-interactive action execution and migration paths
6. `1005`: proof, docs, and closeout

## Ready Chain

- `1000` is complete
- `1001` is complete
- `1002` is complete
- `1003` is complete
- `1004` is complete
- `1005` is complete
- later cards must not start before the checklist model exists

## Auto-Continuation Envelope

Auto-start is enabled while:

- the work stays inside `effigy init`
- new setup jobs are backed by real command surfaces
- JSON contracts stay explicit and additive
- non-TTY behavior remains deterministic

Stop and replan if:

- a meaningful setup job needs a new product surface not currently shipped
- the TTY flow starts requiring richer input than bounded yes/no prompts
- the non-interactive action model becomes too ambiguous for stable agent use

## Acceptance

This lane is complete when:

- TTY `effigy init` behaves as a bounded setup wizard
- checklist mode exposes the same action surface to agents
- non-interactive action execution can drive the same setup jobs explicitly
- docs and proofs are complete
- no active ready card remains

## Next Task

No active ready card.
