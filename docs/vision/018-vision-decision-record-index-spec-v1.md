# 018 Vision Decision Record Index Spec v1

Status: Draft
Owner: Platform Lead
Purpose: define how decision records are indexed so strategic tradeoffs are discoverable, sortable, and reviewable.

## 1. Index Goals

1. Keep decision records searchable by tag, owner, and status.
2. Support governance reviews with minimal manual reconstruction.
3. Preserve links between decisions, exceptions, and risks.

## 2. Required Index Fields

Each decision record entry should include:

1. `Decision ID`: `D-YYYY-NN`.
2. `Date`: decision date.
3. `Title`: short summary.
4. `Tags`: impacted vision tags.
5. `Owner`: accountable role.
6. `Status`: `Open`, `Stabilized`, `Reversed`, or `Closed`.
7. `Reversal Condition`: short trigger phrase.
8. `Exception Link`: related exception ID if applicable.
9. `Risk Link`: related risk ID if applicable.
10. `Record Path`: link to full decision record.

## 3. Index Template

```md
# Vision Decision Record Index

| Decision ID | Date | Title | Tags | Owner | Status | Reversal Condition | Exception | Risk | Record Path |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| D-2026-01 | 2026-03-05 | Preserve schema compatibility during routing refactor | CONTRACT,ROUTE | Platform Lead | Open | contract regression in CI | VE-2026-02 | VR-02 | docs/vision/decisions/D-2026-01.md |
```

## 4. Status Rules

1. `Open`: decision applied but still under active observation.
2. `Stabilized`: decision behavior has held across two review cycles.
3. `Reversed`: decision explicitly rolled back due to trigger.
4. `Closed`: decision complete with no further monitoring required.

## 5. Quality Rules

1. Keep index sorted by date descending, then decision ID.
2. Every `Open` entry must have a review checkpoint date in the full record.
3. Every `Reversed` entry must link to replacement decision or mitigation.
4. Status changes must be reflected in governance reviews (`009`).

## Next Task

Update the index when decision status changes at
[`governance/decision-record-index.md`](./governance/decision-record-index.md).
