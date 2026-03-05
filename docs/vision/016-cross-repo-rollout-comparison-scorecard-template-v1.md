# 016 Cross-Repo Rollout Comparison Scorecard Template v1

Status: Draft
Owner: Platform Lead + Repo Maintainers
Purpose: provide a consistent format for comparing vision adoption posture across repositories.

## 1. Template Intent

1. Make cross-repo maturity and risk differences visible in one artifact.
2. Prioritize platform-level interventions based on comparable signals.
3. Track improvement trends across review windows.

## 2. Comparison Dimensions

Score each repository on:

1. Maturity stage per vision tag (`007`, `010`).
2. Active strategic risks (`004`).
3. Exception burden and expiry pressure (`005`).
4. Recent movement against SLO/target envelopes (`003`).

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

Create the first populated cross-repo scorecard draft for Effigy-adjacent repositories using currently available maturity and exception signals.
