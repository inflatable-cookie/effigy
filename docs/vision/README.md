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
7. Enforce release quality through contract checks, validation logs, and repeatable gates.
8. Favor modular boundaries over feature-local shortcuts to keep iteration speed sustainable.
9. Keep docs and contracts synchronized so operator guidance matches runtime behavior.
10. Compete on reliability and clarity, not just command count.

## Vision Artifacts

1. [001-effigy-runner-blueprint-v1](./001-effigy-runner-blueprint-v1.md)
2. [002-refocus-matrix-v1](./002-refocus-matrix-v1.md)
3. [003-vision-metrics-and-slos-v1](./003-vision-metrics-and-slos-v1.md)
4. [004-vision-risk-register-v1](./004-vision-risk-register-v1.md)
5. [005-vision-exception-and-deviation-policy-v1](./005-vision-exception-and-deviation-policy-v1.md)
6. [006-vision-governance-and-operating-rhythm-v1](./006-vision-governance-and-operating-rhythm-v1.md)
7. [007-vision-adoption-and-maturity-model-v1](./007-vision-adoption-and-maturity-model-v1.md)
8. [008-vision-decision-principles-v1](./008-vision-decision-principles-v1.md)
9. [009-vision-governance-review-template-v1](./009-vision-governance-review-template-v1.md)
10. [010-vision-repository-maturity-scorecard-template-v1](./010-vision-repository-maturity-scorecard-template-v1.md)
11. [011-vision-communications-playbook-v1](./011-vision-communications-playbook-v1.md)
12. [012-vision-tag-and-terminology-canon-v1](./012-vision-tag-and-terminology-canon-v1.md)
13. [013-cross-repo-vision-adoption-playbook-v1](./013-cross-repo-vision-adoption-playbook-v1.md)
14. [014-vision-artifact-lifecycle-policy-v1](./014-vision-artifact-lifecycle-policy-v1.md)
15. [015-vision-decision-record-template-v1](./015-vision-decision-record-template-v1.md)
16. [016-cross-repo-rollout-comparison-scorecard-template-v1](./016-cross-repo-rollout-comparison-scorecard-template-v1.md)
17. [017-vision-artifact-status-register-spec-v1](./017-vision-artifact-status-register-spec-v1.md)
18. [018-vision-decision-record-index-spec-v1](./018-vision-decision-record-index-spec-v1.md)
19. [019-effigy-vision-maturity-baseline-v1](./019-effigy-vision-maturity-baseline-v1.md)
20. [020-strategic-runway-atlas-v1](./020-strategic-runway-atlas-v1.md)

## Governance

Live registers and decision indexes:

- [`governance/README.md`](./governance/README.md)
- [`governance/artifact-status-register.md`](./governance/artifact-status-register.md)
- [`governance/decision-record-index.md`](./governance/decision-record-index.md)
- [`decisions/`](./decisions/) — full decision record bodies

## Vision History

Operational rollout checklists and closeouts live in:

- [`./history/README.md`](./history/README.md)

## Working Rule

Vision docs define directional constraints and target envelopes only.
Architecture, roadmap, guides, and logs should align with these constraints unless a deliberate exception is recorded.

## Next Task

Return to official catalog-pack publication planning under architecture
`026` and contract `043`. The second governance review remains due by
2026-09-17.
