# Vision Risks

Use these as persistent risk categories when changing the product. A current
incident, task, or owner-specific mitigation lives with the relevant Queue
work; this file holds the reason each risk matters and its detection signal.

| ID | Risk | Signal | Impact | Guardrail |
| --- | --- | --- | --- | --- |
| VR-01 | selector complexity outruns routing clarity | more ambiguous-resolution incidents or inconsistent evidence | operators retry, agents guess, trust falls | deterministic precedence and candidate/remediation tests |
| VR-02 | JSON behavior outruns schema and docs | runtime schema changes without synchronized index/examples | integrations break silently | same-PR contract and JSON checks |
| VR-03 | diagnostics become verbose but less actionable | failures show detail without a usable next step | repair time rises | remediation-first output and sampled review |
| VR-04 | feature growth concentrates responsibilities | modules acquire several unrelated reasons to change | refactors get expensive and regressions rise | semantic owners, package map, focused boundary scans |
| VR-05 | release gates become negotiable under pressure | manual bypass proposals or repeated unproved candidates | shipped surface loses reliability | explicit human release authority, exact-head gates, no re-tagging failed releases |

Reassess a category when a signal appears in a PR, a consumer failure, or a
release gate. Two comparable worsening observations call for a product or
planning response. A contract or release gate failure calls for immediate
repair before promotion. Historical risk reviews and stage scorecards stay in
Git history; no periodic status mirror is maintained here.

The unresolved [per-worktree gateway hostname lead](../../triage/20260917-230600-per-worktree-container-identity-default.md)
is a concrete instance of identity/routing risk, not a settled convention.
