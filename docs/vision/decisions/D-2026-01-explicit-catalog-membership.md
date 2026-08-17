# Vision Decision Record — D-2026-01

Context
- Date: 2026-08-10
- Owner: Platform Lead
- Scope: Effigy catalog routing and membership
- Tags: ROUTE, CONTRACT, MAINT

Decision
- Summary: Root-owned explicit catalog membership replaces ambient descendant discovery.
- Principle(s): Determinism over convenience (`008`); fail loudly on ambiguity (`001`).
- Chosen Option: Typed `[catalog.members]` schema with routing cutover and discovery deletion.

Alternatives Considered
- Option A: Keep ambient discovery with stronger diagnostics — rejected because undeclared catalogs remain a silent footgun.
- Option B: Hybrid ambient fallback for migration — rejected because it prolongs nondeterministic routing.

Impact
- Positive: predictable task surfaces; explicit mount semantics; cleaner doctor diagnostics.
- Risk: migration burden for repos relying on implicit discovery.
- Compatibility Effect: medium — requires manifest updates; no silent behavior preservation.

Controls
- Mitigation: migration proof in card `1075`; contract `037`; archived spec `101`.
- Reversal Condition: undeclared nested catalogs reappear in effective routing without operator opt-in.
- Exit Plan: N/A — stabilized after two review cycles if no regression signals.

Traceability
- Related Exception: none
- Related Risk: VR-01
- Related Artifacts: [`g08.028`](../../roadmaps/g08/028-explicit-catalog-membership.md), [`10-105636-explicit-catalog-membership-closeout.md`](../../logs/2026-08/10-105636-explicit-catalog-membership-closeout.md)

Review checkpoint: 2026-09-17
