# 008 Vision Decision Principles v1

Status: Draft
Owner: Platform Lead + Maintainers
Purpose: codify default tradeoff rules when delivery pressure conflicts with reliability, compatibility, or clarity goals.

## 1. Principle Set

1. Reliability over speed for default behavior.
2. Compatibility over convenience for machine-readable interfaces.
3. Explicitness over implicit fallback for routing and deferral.
4. Reversibility over risky one-way changes in release-critical paths.
5. Actionability over verbosity in operator-facing failures.

## 2. Decision Matrix

| Pressure | Preferred Choice | Disallowed Shortcut |
| --- | --- | --- |
| Ship date risk | narrow scope with preserved contract behavior | unversioned schema/output changes |
| Performance regressions | profile-guided optimization with parity tests | removing explainability/evidence fields |
| Complex selector edge cases | fail with remediation hints and candidate evidence | silent fallback to non-deterministic selection |
| Release incident mitigation | reversible guardrails with rollback plan | undocumented permanent bypasses |
| Docs drift | same-PR minimum contract/doc alignment | deferred undocumented behavior changes |

## 3. Tie-Break Rules

1. If a change helps speed but harms contract stability, block until compatibility plan exists.
2. If a change simplifies code but reduces operator diagnostics, require remediation design first.
3. If a change improves one vision tag while degrading another, include explicit net-impact rationale.
4. If urgency requires deviation, open a time-bounded exception record (`005`).

## 4. Required Decision Evidence

Every material tradeoff decision should record:

1. Affected vision tags.
2. Chosen option and rejected alternatives.
3. Short-term risk and mitigation.
4. Reversal conditions and owner.

## 5. Governance Integration

1. Use this document during release readiness reviews (`006` cadence).
2. Reference these principles in exception approvals (`005` policy).
3. Include principle references in roadmap acceptance criteria when tradeoffs are non-trivial.

## Next Task

Create a compact decision record template so tradeoff decisions are documented consistently in roadmap and release artifacts.
