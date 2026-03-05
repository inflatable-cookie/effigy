# 005 Vision Exception and Deviation Policy v1

Status: Draft
Owner: Platform + Repo Maintainers
Purpose: define how teams can temporarily diverge from vision constraints without normalizing drift.

## 1. Policy Intent

1. Keep vision constraints durable while still allowing urgent delivery when needed.
2. Make every deviation explicit, time-bounded, and owned.
3. Ensure exceptions produce follow-up work, not silent permanent divergence.

## 2. What Requires an Exception

An exception is required when a planned change temporarily violates one or more vision constraints in:

1. Deterministic routing behavior (`ROUTE`).
2. JSON contract compatibility or schema governance (`CONTRACT`).
3. Operator actionability or diagnostic expectations (`OPERATE`).
4. Modularity and maintainability boundaries (`MAINT`).
5. Release gate or rollback reliability posture (`RELEASE`).

## 3. Exception Record Contract

Each exception must be documented in one short record containing:

1. `Exception ID`: unique identifier (`VE-YYYY-NN` suggested).
2. `Scope`: repos, commands, or docs affected.
3. `Tag(s)`: impacted vision tags.
4. `Reason`: concrete delivery pressure or technical blocker.
5. `Risk`: expected downside while exception is active.
6. `Mitigation`: guardrail(s) during exception window.
7. `Expiry`: explicit date or milestone for removal.
8. `Owner`: accountable maintainer role.
9. `Exit Plan`: required restoration steps.

## 4. Approval and Duration Rules

1. Any exception affecting `CONTRACT` or `RELEASE` requires explicit maintainer sign-off in the same PR.
2. Exception windows should be short; default maximum is one release cycle unless renewed.
3. Renewals require evidence that mitigation was effective and exit work is scheduled.
4. Expired exceptions are treated as policy violations until resolved or renewed.

## 5. Reporting and Visibility

1. Active exceptions should be summarized in release-readiness or validation reports.
2. Reports should include exception count, nearing-expiry items, and overdue items.
3. Closed exceptions should link to evidence showing restoration completed.

## 6. Non-Negotiable Constraints

1. No untracked exceptions: if it is not recorded, it is not approved.
2. No exception may remove all operator remediation guidance from failure paths.
3. No exception may permanently bypass contract checks without a replacement gate.
4. No exception may remain open indefinitely without escalation.

## 7. Adoption Steps

1. Add exception section templates to release and validation report workflows.
2. Add a docs QA check that flags missing expiry or owner fields in active exceptions.
3. Add a periodic review cadence aligned with release checkpoints.

## Next Task

Create a canonical exception log location and template so exception records are consistently authored and reviewed.
