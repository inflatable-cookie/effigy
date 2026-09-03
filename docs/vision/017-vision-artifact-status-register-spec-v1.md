# 017 Vision Artifact Status Register Spec v1

Status: Active
Owner: Docs Owners + Platform Lead
Purpose: define a compact register format that tracks lifecycle status and ownership for all active vision artifacts.

## 1. Register Goals

1. Provide one snapshot of active strategy artifacts and their health.
2. Make lifecycle state and accountability explicit (`014`).
3. Reduce stale documents staying active without review.

## 2. Required Fields

Each artifact row should include:

1. `ID`: numeric artifact identifier (for example `001`).
2. `Title`: short canonical title.
3. `State`: `Draft`, `Active`, `Superseded`, or `Archived`.
4. `Owner`: accountable role.
5. `Review Cadence`: weekly, monthly, quarterly, or release.
6. `Last Reviewed`: date in `YYYY-MM-DD`.
7. `Successor`: successor artifact ID/path when superseded.
8. `Notes`: one-line rationale or risk note.

## 3. Register Template

```md
# Vision Artifact Status Register — <YYYY-MM-DD>

| ID | Title | State | Owner | Review Cadence | Last Reviewed | Successor | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 001 | Effigy Runner Blueprint v1 | Active | Platform + Runtime | Quarterly | 2026-03-05 | - | Core strategy anchor |
| 014 | Vision Artifact Lifecycle Policy v1 | Draft | Platform + Docs | Monthly | 2026-03-05 | - | Pending adoption in review cadence |
```

## 4. Operational Rules

1. Keep rows sorted by numeric artifact ID.
2. Any `Superseded` row must include a successor.
3. Any `Archived` row should include a pointer to history location.
4. Any `Draft` older than two review cycles must be actioned (activate, supersede, or archive).

## 5. Governance Integration

1. Review this register in monthly and quarterly governance cycles (`006`).
2. Reference state changes in governance review output (`009`).
3. Use this register to decide index cleanup in `docs/vision/README.md`.

## Next Task

Update the register on the monthly cadence at
[`governance/artifact-status-register.md`](./governance/artifact-status-register.md).
