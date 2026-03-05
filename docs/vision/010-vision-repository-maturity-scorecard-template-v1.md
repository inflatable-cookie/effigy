# 010 Vision Repository Maturity Scorecard Template v1

Status: Draft
Owner: Platform + Repo Maintainers
Purpose: standardize how repositories are assessed against the vision maturity model (`007`).

## 1. Scorecard Dimensions

Each repository is scored across:

1. `ROUTE` deterministic routing and explainability.
2. `CONTRACT` JSON/schema compatibility and drift control.
3. `OPERATE` operator actionability and workflow ergonomics.
4. `MAINT` modularity and refactor safety.
5. `RELEASE` gate repeatability and rollback readiness.

## 2. Stage Scale

Use integer stage values from `0` to `4` aligned to `007`:

1. `0`: Ad Hoc
2. `1`: Baseline Aligned
3. `2`: Operationally Reliable
4. `3`: Strategically Governed
5. `4`: Scaled and Self-Correcting

## 3. Scorecard Template

```md
# Vision Maturity Scorecard — <repo> — <YYYY-MM-DD>

Summary
- Overall Stage: <0-4>
- Movement: <up|flat|down> vs previous review
- Primary Constraint: <short phrase>

Dimension Scores
| Tag | Stage | Evidence | Gap | Owner |
| --- | --- | --- | --- | --- |
| ROUTE | <0-4> | <tests/outputs/reports> | <what blocks next stage> | <role> |
| CONTRACT | <0-4> | <checks/docs/schema refs> | <what blocks next stage> | <role> |
| OPERATE | <0-4> | <diagnostics/workflow evidence> | <what blocks next stage> | <role> |
| MAINT | <0-4> | <module/refactor evidence> | <what blocks next stage> | <role> |
| RELEASE | <0-4> | <gate/rollback evidence> | <what blocks next stage> | <role> |

Promotion Plan
- Target Stage: <N>
- Required Changes: <top 3 actions>
- Review Date: <date>
```

## 4. Scoring Guardrails

1. Do not assign a stage without concrete evidence reference.
2. Do not claim promotion if any mandatory prior-stage criteria are unmet.
3. Keep gap descriptions specific and action-oriented.
4. Regressions require explicit remediation tasks in the next planning cycle.

## 5. Usage Pattern

1. Use scorecards in quarterly planning and major release retrospectives.
2. Reference scorecard deltas in roadmap prioritization.
3. Use repeated low scores to trigger governance escalation (`006`).

## Next Task

Create an initial scorecard for Effigy itself using current docs, checks, and release evidence as baseline inputs.
