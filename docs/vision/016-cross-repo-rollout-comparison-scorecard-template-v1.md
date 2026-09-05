# 016 Cross-Repo Rollout Comparison Scorecard Template v1

Status: Draft
Owner: Platform Lead + Repo Maintainers
Purpose: provide a consistent format for comparing vision adoption posture across repositories.

## 1. Template Intent

1. Make cross-repo maturity and risk differences visible in one artifact.
2. Prioritize platform-level interventions based on comparable signals.
3. Track improvement trends across review windows.

## 2. Comparison Dimensions

Score each **platform repository** on:

1. Maturity stage per vision tag (`007`, `010`).
2. Active strategic risks (`004`).
3. Exception burden and expiry pressure (`005`).
4. Recent movement against SLO/target envelopes (`003`).

Record each **consumer repository** on the adoption posture from `007`
section 6 instead of a stage: one row with `ROUTE`, `DOCS`, `NORTHSTAR`,
`PARITY`, and `RETRIEVAL` as `pass`, `fail`, `unavailable`, or `n/a`, an
overall posture of `aligned`, `drifted`, or `unobserved`, and the frozen
consumer revision. Consumer rows carry no stage, risk count, or exception
count, and are never compared with platform rows on one scale.

## 3. Scorecard Template

```md
# Cross-Repo Vision Rollout Scorecard — <YYYY-MM-DD>

Window
- Review Cadence: <monthly|quarterly>
- Repositories: <list>

Comparison Table
| Repo | Overall Stage | ROUTE | CONTRACT | OPERATE | MAINT | RELEASE | Active Risks | Active Exceptions | Overdue Exceptions | Recent Movement |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| <repo-a> | <0-4> | <0-4> | <0-4> | <0-4> | <0-4> | <0-4> | <count> | <count> | <count> | <up|flat|down> |
| <repo-b> | <0-4> | <0-4> | <0-4> | <0-4> | <0-4> | <0-4> | <count> | <count> | <count> | <up|flat|down> |

Priority Actions
- Repo: <name>
- Constraint: <primary blocker>
- Action: <specific intervention>
- Owner: <role>
- Due: <date>
```

## 4. Interpretation Rules

1. Prioritize repositories with low stage plus high exception/risk pressure.
2. Treat repeated downward movement as escalation trigger.
3. Validate outlier scores with linked evidence before actioning.
4. Keep comparisons relative to the same review window and criteria.

## 5. Governance Usage

1. Use in quarterly strategy reviews and cross-repo planning.
2. Reference actions in roadmap promotions and release readiness discussions.
3. Re-run after major policy or architecture changes to verify impact.

## Next Task

Publish the next scorecard with Acowtancy on the adoption-posture row format
from `007` section 6; the 2026-09-03 scorecard's unknown stage cells are
superseded by that format, not by new stage claims.
