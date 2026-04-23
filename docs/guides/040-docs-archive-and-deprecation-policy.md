# 040 - Docs Archive and Deprecation Policy

Use this policy to retire, merge, or demote stale guides while preserving link continuity.

## 1) When to Deprecate a Guide

Deprecate when one or more are true:
- behavior described is no longer supported
- guide content is fully superseded by a newer canonical guide
- guide duplicates content now maintained elsewhere
- guide remains useful only for historical context

Do not deprecate solely due to age.

## 2) Deprecation Markers

At top of deprecated guide, add:

```md
> Status: Deprecated
> Superseded by: `<new-guide>.md`
> Kept for: historical context / legacy migration references
```

If no direct replacement exists, state that explicitly.

## 3) Archive vs Supplemental

Use **Supplemental** placement when:
- guide is still occasionally useful
- readers may need it for legacy environments

Use **Archive** placement when:
- guide should not be used for current operations
- content exists only for historical traceability

Current practice:
- archived guides live in `docs/guides/archive/` with a short index at
  `docs/guides/archive/README.md`
- the hub README's `Archive` section lists inbound links one layer deep so old
  references still resolve without cluttering primary onboarding

## 4) Entry-Point Rules

For deprecated docs:
- remove from primary onboarding sequences (`README.md`, Start Here)
- keep a single discoverable link in:
  - `docs/README.md` (supplemental/archive section)
  - `docs/guides/README.md` (supplemental/archive section)

Avoid repeating deprecated links in multiple top sections.

## 5) Safe Merge Procedure

When merging Guide A into Guide B:
1. move unique content from A to B
2. add deprecation marker in A pointing to B
3. keep A file path stable initially (no immediate deletion)
4. update indexes to demote A to supplemental/archive
5. run link checker

Validation:

```sh
effigy docs check-links README.md $(find docs -name '*.md' | sort)
```

## 6) Deletion Policy

A deprecated guide may be deleted only when:
- no inbound references remain in repo docs, and
- at least one release cycle has passed with deprecation marker in place, and
- replacement coverage is confirmed

Before deletion:
- verify with repo-wide search for file path references
- include deletion note in a docs changelog log

## 7) Archive Index Recommendation

Maintain a lightweight archive list in one place (recommended: `docs/guides/README.md`):
- deprecated/superseded guides
- reason for demotion
- replacement guide link

## 8) PR Checklist for Deprecation/Archive Changes

```md
## Docs Deprecation/Archive Checklist
- [ ] Deprecation marker added (if keeping file)
- [ ] Replacement guide linked (or explicit none)
- [ ] Removed from primary onboarding paths
- [ ] Added to supplemental/archive section
- [ ] `effigy docs check-links README.md $(find docs -name '*.md' | sort)` passed
```

## Expected Outcome

- deprecated guides remain discoverable without polluting primary onboarding paths
- replacement guidance stays explicit and link-safe during transitions
- archive/deprecation changes are auditable through checklist-driven PRs

## Related Guides

- [`archive/README.md`](./archive/README.md)
- [`archive/031-docs-navigation-cleanup.md`](./archive/031-docs-navigation-cleanup.md)
- [`archive/032-docs-consistency-sweep-and-changelog.md`](./archive/032-docs-consistency-sweep-and-changelog.md)
- [`035-guide-ownership-and-update-triggers.md`](./035-guide-ownership-and-update-triggers.md)
- [`039-docs-drift-monitoring.md`](./039-docs-drift-monitoring.md)

## Next Step

When deprecating a guide, record the decision and replacement path in the next dated log under `docs/logs/YYYY-MM/` and cross-link it from index updates.
