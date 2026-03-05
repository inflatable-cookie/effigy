# Vision Index

This folder captures Effigy's long-horizon product and platform direction.
It answers one question: if Effigy is the orchestration spine across many repos, what constraints and quality bars must always hold as features expand?

## Core Position

Effigy should be a deterministic, automation-grade, operator-first orchestration platform:

1. Resolve tasks deterministically across nested catalogs and mixed workspaces.
2. Keep one stable command envelope for machine consumers (`effigy.command.v1`), with versioned payload contracts.
3. Treat explainability as a product requirement (selection evidence, deferral reasoning, health findings).
4. Make operational safety explicit with lock scopes, policy controls, and predictable failure semantics.
5. Keep onboarding and migration built-in (`init`, `migrate`) so adoption does not require bespoke scripts first.
6. Design text, JSON, and TUI outputs as parallel surfaces with the same underlying facts.
7. Enforce release quality through contract checks, validation reports, and repeatable gates.
8. Favor modular boundaries over feature-local shortcuts to keep iteration speed sustainable.
9. Keep docs and contracts synchronized so operator guidance matches runtime behavior.
10. Compete on reliability and clarity, not just command count.

## Vision Artifacts

1. [001-effigy-runner-blueprint-v1](./001-effigy-runner-blueprint-v1.md)
2. [002-refocus-matrix-v1](./002-refocus-matrix-v1.md)
3. [003-vision-metrics-and-slos-v1](./003-vision-metrics-and-slos-v1.md)
4. [004-vision-risk-register-v1](./004-vision-risk-register-v1.md)

## Vision History

Operational rollout checklists and closeouts live in:

- [`../reports/vision-history/README.md`](../reports/vision-history/README.md)

## Working Rule

Vision docs define directional constraints and target envelopes only.
Architecture, roadmap, guides, and reports should align with these constraints unless a deliberate exception is recorded.

## Next Task

Define a high-level exception/deviation policy document that explains when and how teams may temporarily diverge from vision constraints.
