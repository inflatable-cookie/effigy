# Vision Decision Record — D-2026-05

Context
- Date: 2026-09-03
- Owner: Operator + Platform
- Scope: next Effigy planning horizon after the second governance cycle
- Tags: RELEASE, OPERATE, CONTRACT, ROUTE

Decision
- Summary: select Theme 3, consumer adoption cohort replay, with Acowtancy as
  the first current non-fixture pilot.
- Principle(s): evidence before platform widening (`008`); prove shared
  semantics in a real consumer before changing the product (`001`, `013`).
- Chosen Option: freeze Acowtancy `main`, run its existing Northstar and docs
  contract gates with current Effigy, publish an evidence-backed comparison
  scorecard, and repair only demonstrated Effigy-owned starter or guide drift.

Alternatives Considered
- Option A: hold planning after governance cycle two — rejected because the
  existing consumer contract and Acowtancy gates make a bounded replay ready.
- Option B: Theme 5 release-candidate hardening — deferred because release
  remains a separate operator-gated mutation and consumer evidence is the
  lower-risk next step.
- Option C: use Northstar or Bovine Accelerator as the first pilot — retained
  for later cohort expansion; Acowtancy offers the strongest current
  nested-catalog and docs-authority evidence without requiring product edits.

Impact
- Positive: tests the portable contract against current real-repository state
  and supplies the first populated cross-repo scorecard.
- Risk: the replay could drift into Acowtancy product work or treat one pilot as
  universal evidence.
- Compatibility Effect: none; the first card is evidence and narrowly bounded
  Effigy documentation/starter reconciliation only.

Controls
- Mitigation: strict spec `118`, roadmap `g09.003`, card `1111`, a frozen
  consumer SHA, no Acowtancy writes, and stop conditions on product or runtime
  defects outside Effigy-owned contract guidance.
- Reversal Condition: Acowtancy cannot exercise the portable contract without
  repo-specific product mutation, or its evidence cannot distinguish Effigy
  drift from consumer-local policy.
- Exit Plan: close the first replay with a populated scorecard and explicit
  cohort-expansion recommendation, or return to planning with the failed
  assumption named.

Traceability
- Related Exception: none
- Related Risk: VR-01, VR-02, VR-03
- Related Artifacts: [`013`](../013-cross-repo-vision-adoption-playbook-v1.md),
  [`016`](../016-cross-repo-rollout-comparison-scorecard-template-v1.md),
  [`guide 056`](../../guides/056-northstar-effigy-consumer-repo-contract.md),
  [`g09.003`](../../roadmaps/g09/003-acowtancy-consumer-adoption-replay.md),
  [`strict spec 118`](../../specs/archive/118-acowtancy-consumer-adoption-replay-strict-lane.md)

Review checkpoint: card `1111` closeout or the next monthly governance review,
whichever comes first.
