# 004 Vision Risk Register v1

Status: Draft
Owner: Platform + Maintainers
Purpose: maintain a concise, high-level risk view for threats to Effigy's vision outcomes.

## 1. Risk Model

Each risk is tracked with:

1. `Tag`: primary vision tag affected.
2. `Signal`: concrete indicator the risk is materializing.
3. `Impact`: what degrades if untreated.
4. `Mitigation`: current planned response.
5. `Owner`: accountable role.
6. `Review`: cadence for reassessment.

## 2. Active Risks

| ID | Tag | Risk | Signal | Impact | Mitigation | Owner | Review |
| --- | --- | --- | --- | --- | --- | --- | --- |
| VR-01 | `ROUTE` | selector complexity grows faster than routing clarity | increase in ambiguous-resolution incidents | operator trust drops and retries rise | preserve deterministic precedence tests and explain-mode evidence checks | Runtime | weekly |
| VR-02 | `CONTRACT` | JSON surface changes outpace schema/docs governance | schema changes merged without synchronized docs/index updates | CI/tooling integrations break silently | enforce contract checks + same-PR docs/index updates | Platform | per PR |
| VR-03 | `OPERATE` | diagnostics become verbose but less actionable | failures show detail without explicit next steps | troubleshooting time increases | keep remediation-first output standard and sample review | Maintainers | monthly |
| VR-04 | `MAINT` | feature growth reintroduces concentrated complexity hotspots | large files/modules regain multi-responsibility patterns | refactor cost and regression risk increase | continue modular extraction and hotspot tracking in roadmap planning | Platform | monthly |
| VR-05 | `RELEASE` | release gate bypasses become normalized under schedule pressure | manual overrides increase in release cycles | reliability posture degrades over time | require post-override incident note and tightening action | Release Owner | per release |

## 3. Escalation Rules

1. Escalate immediately when two consecutive review windows show worsening trend for the same risk.
2. Escalate when a risk directly causes contract or release gate failure.
3. Escalate when mitigation owners or review cadence are undefined.

## 4. Closure Rules

1. A risk can move to monitoring-only when signals remain below threshold for two review cycles.
2. Closed risks should retain historical note in logs history with closure rationale.
3. Reopened risks keep original ID and append a reopen reason/date.

## 5. Governance Notes

1. This register is strategy-level; implementation details belong in roadmap/log artifacts.
2. Risk list should stay short and high signal.
3. Add new risks only when they materially threaten one or more vision tags.

## Next Task

Add risk thresholds and trigger examples for each active risk so escalation criteria are testable and consistent.
