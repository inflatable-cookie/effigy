# 006 Vision Governance and Operating Rhythm v1

Status: Draft
Owner: Platform Lead + Maintainers
Purpose: define the recurring governance cadence that keeps vision constraints active in day-to-day delivery.

## 1. Governance Goals

1. Keep vision constraints continuously visible in planning, implementation, and release decisions.
2. Detect drift early through lightweight recurring reviews.
3. Tie corrective action directly to owners and timelines.

## 2. Operating Rhythm

### Weekly (Delivery Health)

1. Review routing/actionability signals and open high-severity regressions.
2. Confirm no unowned exceptions were introduced.
3. Check that active roadmap work cites relevant vision tags.

### Monthly (Quality and Drift)

1. Review metric trends against `003` targets and SLO envelopes.
2. Reassess active risk entries from `004`.
3. Audit contract/docs synchronization drift and remediation completion.

### Per Release (Readiness and Reliability)

1. Validate release gates and rollback posture for the release candidate.
2. Review active exceptions for expiry risk and carry-forward justification.
3. Record vision target deltas in release artifacts.

### Quarterly (Strategic Recalibration)

1. Re-evaluate whether current target envelopes still represent the intended quality bar.
2. Retire stale risks and add newly material strategic risks.
3. Confirm vision docs still map to active product direction.

## 3. Roles and Accountability

1. `Platform Lead`: final owner of vision consistency and threshold changes.
2. `Runtime Maintainers`: owners of routing, diagnostics, and operator workflow quality.
3. `Release Owner`: owner of gate integrity, exception carryover, and rollback readiness.
4. `Docs Owners`: owners of index accuracy, terminology consistency, and policy cross-links.

## 4. Decision Inputs

Each cadence review should consume:

1. Current metric snapshot and trend deltas.
2. Active risk and exception lists.
3. Gate outcomes from recent PRs/releases.
4. Material user/operator incident notes when available.

## 5. Escalation Conditions

1. Two consecutive review cycles below SLO for the same vision tag.
2. Repeated gate bypasses without corrective action.
3. Multiple expired exceptions with no close plan.
4. Contract drift that reaches release branches.

## 6. Governance Artifacts

1. Vision index (`docs/vision/README.md`) as canonical map.
2. Metrics, risk, and exception policy docs as stable reference sources.
3. Reports history for execution evidence, separate from strategy docs.

## Next Task

Define a compact governance review template that captures metrics, risks, exceptions, and actions in one page.
