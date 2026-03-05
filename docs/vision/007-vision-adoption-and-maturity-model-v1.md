# 007 Vision Adoption and Maturity Model v1

Status: Draft
Owner: Platform + Repository Maintainers
Purpose: define maturity stages for adopting Effigy vision standards across repositories and teams.

## 1. Model Intent

1. Provide a shared language for "where a repo is" on vision adoption.
2. Prioritize interventions based on maturity gaps.
3. Avoid binary pass/fail framing for long-running migration work.

## 2. Maturity Stages

### Stage 0: Ad Hoc

1. Effigy usage exists but behavior and docs are inconsistent.
2. Contract checks are partial or absent.
3. Routing and failure outputs vary by workflow.

### Stage 1: Baseline Aligned

1. Core workflows (`tasks`, `doctor`, `test --plan`) follow documented behavior.
2. JSON contract coverage exists for primary command paths.
3. Basic docs QA and release gate checks are in place.

### Stage 2: Operationally Reliable

1. Deterministic routing evidence is test-covered and monitored.
2. Failure modes consistently provide actionable remediation.
3. Release gates run repeatably with low manual bypass frequency.

### Stage 3: Strategically Governed

1. Metrics and SLOs are tracked with regular cadence and ownership.
2. Risk and exception governance is active and time-bounded.
3. Roadmap and logs explicitly reference vision-tag movement.

### Stage 4: Scaled and Self-Correcting

1. Drift is detected early and corrected without large audit bursts.
2. Cross-repo patterns stay aligned with minimal bespoke process.
3. Vision docs are updated proactively as platform scope evolves.

## 3. Assessment Dimensions

Use these dimensions to determine stage per repository:

1. `Routing Determinism` (`ROUTE`)
2. `Contract Stability` (`CONTRACT`)
3. `Operator Actionability` (`OPERATE`)
4. `Maintainability Posture` (`MAINT`)
5. `Release Reliability` (`RELEASE`)

## 4. Advancement Criteria

1. Advancement requires meeting all prior stage criteria.
2. Temporary exceptions are allowed only if they are recorded and time-bounded.
3. A stage claim should include evidence references in logs or check outputs.

## 5. Recommended Use

1. Use stage targeting in roadmap acceptance criteria.
2. Use stage snapshots in quarterly planning and release retrospectives.
3. Use stage regression as an escalation signal for governance reviews.

## Next Task

Create a repository scorecard template that maps each maturity dimension to observable evidence and stage thresholds.
