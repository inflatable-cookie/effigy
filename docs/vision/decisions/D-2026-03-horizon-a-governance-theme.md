# Vision Decision Record — D-2026-03

Context
- Date: 2026-08-17
- Owner: Platform Lead
- Scope: Effigy planning horizon after `g08` closeout
- Tags: MAINT, RELEASE, OPERATE

Decision
- Summary: Horizon A Theme 1 (vision governance operationalization) is the next strict lane after Atlas refresh.
- Principle(s): Evidence-driven delivery (`001`); govern before feature sprawl (`019`, `020`).
- Chosen Option: Populate governance registers and run the first review cycle before agent-adoption or release themes.

Alternatives Considered
- Option A: Theme 2 agent-native maintainer experience — deferred; graph/scan adoption benefits from governance baseline.
- Option B: Theme 5 release candidate hardening — deferred; no operator release instruction and governance gap remains.
- Option C: Theme 4 breaking command-surface preview — deferred; requires explicit semver-breaking appetite.

Impact
- Positive: templates `009`/`015`/`017`/`018` become live surfaces; maturity path toward stage 3.
- Risk: slower visible feature velocity while docs/process work lands.
- Compatibility Effect: none — docs-only lane.

Controls
- Mitigation: strict lane `105` with three bounded cards; monthly review cadence from artifact `006`.
- Reversal Condition: governance surfaces abandoned or a second parallel planning authority emerges outside vision/docs.
- Exit Plan: second governance review on 2026-09-17; then operator selects next Horizon theme.

Traceability
- Related Exception: none
- Related Risk: VR-04
- Related Artifacts: [`020-strategic-runway-atlas-v1`](../020-strategic-runway-atlas-v1.md), [`g08.032`](../../roadmaps/g08/032-vision-governance-operationalization.md), [`g09-candidate-themes`](../../roadmaps/backlog/g09-candidate-themes.md)

Review checkpoint: 2026-09-17
