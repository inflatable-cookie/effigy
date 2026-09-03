# 014 Vision Artifact Lifecycle Policy v1

Status: Active
Owner: Platform Lead + Docs Owners
Purpose: define when strategy-level vision documents should be created, revised, superseded, or archived.

## 1. Policy Goals

1. Keep the vision set concise, current, and high signal.
2. Prevent uncontrolled growth of stale or duplicate strategy artifacts.
3. Preserve a clear history trail without cluttering active vision guidance.

## 2. Lifecycle States

1. `Draft`: proposed strategy intent under active shaping.
2. `Active`: approved and expected to guide roadmap/guides/logs.
3. `Superseded`: replaced by a newer artifact with explicit successor reference.
4. `Archived`: moved out of active vision set into history references.

## 3. Create Rules

Create a new vision artifact only when:

1. A strategic gap is not already covered by active artifacts.
2. The topic affects multiple delivery streams or repositories.
3. A stable high-level constraint or model is needed beyond one release cycle.

## 4. Revise Rules

Revise an active artifact when:

1. The core intent remains valid, but thresholds/definitions need updates.
2. Governance cadence reveals drift caused by unclear language.
3. Canonical terminology or tag mappings need correction.

## 5. Supersede Rules

Supersede an artifact when:

1. The new version materially changes structure or strategic framing.
2. Multiple overlapping artifacts can be collapsed into one clearer source.
3. The previous artifact cannot be safely edited without losing historical context.

Superseded artifacts must:

1. Retain original file for traceability.
2. Include a pointer to successor artifact.
3. Be removed from active index listings.

## 6. Archive Rules

Archive when:

1. The document is no longer strategy-relevant for current direction.
2. Its guidance has been fully absorbed into another active artifact.
3. It represents execution closeout details rather than enduring strategy.

Archived strategy artifacts should be referenced from history indexes, not active vision index.

## 7. Ownership and Review

1. Lifecycle decisions require named owner approval.
2. Evaluate lifecycle status quarterly as part of governance rhythm (`006`).
3. Record supersede/archive rationale in review artifacts (`009` template).

## Next Task

Create a lightweight artifact status table template so lifecycle state, owner, and successor links are tracked consistently.
