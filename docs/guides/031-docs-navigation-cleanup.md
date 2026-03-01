# 031 - Docs Navigation Cleanup

This note documents the docs navigation normalization completed in March 2026.

## What Changed

- Reduced newcomer "Start Here" path to a canonical onboarding sequence.
- Kept `028-docs-flow-map.md` as supplemental navigation content instead of primary onboarding.
- Preserved `028-migration-quick-paths.md` as the operational migration guide.
- Added ordering consistency in index lists so numbered guides appear in numeric sequence.

## Numbering Collision Policy

Current collision:
- `028-migration-quick-paths.md`
- `028-docs-flow-map.md`

Policy going forward:
- Do not renumber historical guides in-place unless a dedicated migration pass is approved.
- Prefer adding new guides with the next available number.
- For collisions, keep one guide in primary learning paths and place the other in supplemental/appendix sections.

## Canonical Entry Points

Primary:
- `README.md` (project-level quick start)
- `docs/README.md` (docs system index)
- `docs/guides/README.md` (persona + task-oriented runbooks)

Supplemental:
- `docs/guides/028-docs-flow-map.md`

## Maintenance Checklist

When adding a new guide:
1. Add it to `docs/guides/README.md` topic index.
2. Add it to `docs/README.md` guide index.
3. Add to root `README.md` only if it is newcomer-critical.
4. Avoid duplicate placement in both "Start Here" and long-form topic lists unless intentional.

## Expected Outcome

- newcomers have one clear reading path from root README to operational guides
- legacy or historical navigation aids remain discoverable but non-primary
- index pages stay consistent after new guide additions

## Related Guides

- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)
- [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md)
- [`030-contributor-onboarding-15-minutes.md`](./030-contributor-onboarding-15-minutes.md)

## Next Step

After any index change, record the update in [`032-docs-consistency-sweep-and-changelog.md`](./032-docs-consistency-sweep-and-changelog.md) and re-run docs link checks.
