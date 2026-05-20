# 094 - Codebase Leanness And Boundary Hardening Strict Lane

Roadmap: [`g07.056`](../roadmaps/g07/056-codebase-leanness-and-boundary-hardening-suite.md)
Related planning:
- [`g07.057`](../roadmaps/g07/057-codegraph-language-emitter-deduplication.md)
- [`g07.058`](../roadmaps/g07/058-codegraph-manifest-query-module-decomposition.md)
- [`g07.059`](../roadmaps/g07/059-init-setup-module-boundary-cleanup.md)
- [`g07.060`](../roadmaps/g07/060-json-help-contract-consistency-cleanup.md)
- [`g07.061`](../roadmaps/g07/061-runner-domain-boundary-and-test-fixture-cleanup.md)
- [`g07.062`](../roadmaps/g07/062-crate-boundary-rejustification-and-planning-hygiene.md)
- [`g07.063`](../roadmaps/g07/063-codebase-leanness-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Execute the next codebase leanness suite from the reusable sweep audit without
turning cleanup into a broad rewrite.

## Lane Posture

Posture: `closed-no-ready-card`

This lane exists because recent graph and init work made Effigy more capable,
but also created new maintenance pressure. The work is cleanup with product
discipline: behavior stays stable unless a focused test proves the current
behavior is wrong.

## Hard Boundaries

- no release mutations
- no `.github/workflows/` edits
- no graph storage or public JSON rewrite
- no dynamic language plugins
- no second onboarding command beside `effigy init`
- no crate merge without explicit evidence and a separate decision
- no runner rewrite

## Execution Order

1. `1006`: open the lane and record baseline evidence
2. `1007`: deduplicate codegraph language emitters
3. `1008`: decompose graph manifest/query modules
4. `1009`: split init setup inventory and wizard boundaries
5. `1010`: normalize JSON/report/help conventions
6. `1011`: trim runner domain and test fixture duplication
7. `1012`: review crate boundaries and planning hygiene
8. `1013`: close with proof and residual debt

## Ready Chain

- `1006` is complete
- `1007` is complete
- `1008` is complete
- `1009` is complete
- `1010` is complete
- `1011` is complete
- `1012` is complete
- `1013` is complete
- later cards must not start until the prior card is complete or explicitly
  paused with a clear handoff

## Auto-Continuation Envelope

Auto-start is enabled while:

- work follows the ordered cards
- public CLI and JSON behavior stays stable
- cleanup remains local to the named surface
- focused validation is run before moving to the next surface

Stop and replan if:

- a cleanup requires public contract changes
- a crate merge becomes tempting but not mechanically obvious
- graph behavior changes in a way the current tests cannot explain
- runner extraction starts crossing shell/process glue into domain crates

## Acceptance

This lane is complete when:

- all cards `1006` through `1013` are complete
- scan deltas and tests are recorded
- remaining debt is explicitly deferred or rejected
- no active ready card remains

This lane is now closed. Keep it as a historical execution surface until a
later planning cleanup archives it.

## Next Task

No active ready card.
