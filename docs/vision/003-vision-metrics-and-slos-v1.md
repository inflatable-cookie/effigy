# 003 Vision Metrics and SLOs v1

Status: Draft
Owner: Platform + Runtime
Purpose: define measurable signals for whether Effigy is meeting vision-level quality targets.

## 1. Measurement Principles

1. Metrics should reflect operator and automation outcomes, not just internal activity.
2. SLOs should be specific enough to guide tradeoffs but stable enough to compare over time.
3. Each metric should have a clear owner, source, and reporting cadence.

## 2. Vision Metric Set

| Vision Tag | Metric | Definition | Target Envelope |
| --- | --- | --- | --- |
| `ROUTE` | Deterministic resolution rate | identical selector + context yields identical outcome/evidence | 99.9%+ consistency in controlled regression suites |
| `ROUTE` | Ambiguity remediation quality | ambiguity errors include candidate set + next action | 100% of ambiguity failures include remediation hints |
| `CONTRACT` | Envelope conformance | JSON outputs remain valid `effigy.command.v1` envelopes | 100% conformance in contract CI checks |
| `CONTRACT` | Schema drift lag | time between runtime schema change and docs/index update | same PR or next business day maximum |
| `OPERATE` | Core command responsiveness | `tasks`, `doctor`, `test --plan` completion time on baseline repo | interactive baseline maintained (sub-second target where applicable) |
| `OPERATE` | Diagnostic actionability | failure outputs include explicit next actions | 95%+ of sampled failures judged actionable |
| `MAINT` | High-risk module concentration | number of oversized multi-responsibility modules | trend down quarter-over-quarter |
| `MAINT` | Refactor safety | regressions after structural refactor batches | zero contract regressions in refactor releases |
| `RELEASE` | Release gate repeatability | release/docs quality gates pass rate across target branches | 95%+ pass rate without manual bypass |
| `RELEASE` | Rollback readiness | time to execute rollback playbook in drills/incidents | bounded execution window defined and met |

## 3. SLO Cadence

1. Weekly: operational checks (`ROUTE`, `OPERATE`) in maintenance checkpoints.
2. Monthly: contract and maintainability summaries (`CONTRACT`, `MAINT`).
3. Per release: release readiness and rollback posture (`RELEASE`).
4. Quarterly: trend review and SLO threshold recalibration.

## 4. Reporting Contract

When a metric is reported, include:

1. `Metric`: name and tag.
2. `Observed`: measured value/window.
3. `SLO`: target threshold.
4. `Delta`: movement since previous report.
5. `Action`: follow-up when below target.

## 5. Guardrails

1. Do not optimize a metric in isolation if it degrades another vision tag.
2. Do not change thresholds without documenting rationale and expected impact.
3. Keep metric definitions stable unless behavior meaning has materially changed.

## Next Task

Define the minimum evidence package and source-of-truth files used to compute each metric in release and monthly reports.
