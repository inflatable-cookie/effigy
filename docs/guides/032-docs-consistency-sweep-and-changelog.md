# 032 - Docs Consistency Sweep and Changelog

Date: 2026-03-01

This note captures the docs consistency sweep across primary entry points.

## Scope

Files reviewed/normalized:
- `README.md`
- `docs/README.md`
- `docs/guides/README.md`

Validation performed:

```sh
./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)
```

## Findings

- Markdown links resolved successfully (`link check passed`).
- Numbering collision still exists for two `028-*` guides:
  - `028-migration-quick-paths.md`
  - `028-docs-flow-map.md`
- Collision is now explicitly handled by placement:
  - migration quick paths in primary onboarding paths
  - docs flow map in supplemental sections

## Normalization Changes

- Reduced root `README.md` extended guides to a smaller canonical set.
- Removed `028-docs-flow-map.md` from primary onboarding/start path lists.
- Kept `028-docs-flow-map.md` discoverable as supplemental documentation.
- Added this changelog note to docs indexes for traceability.

## Current Entry-Point Roles

- `README.md`: product-level orientation and quick adoption links.
- `docs/README.md`: docs-system index + broad catalog.
- `docs/guides/README.md`: operational runbook navigation and persona paths.

## Changelog (Docs Navigation)

- Added: `031-docs-navigation-cleanup.md`
- Added: `032-docs-consistency-sweep-and-changelog.md`
- Updated: `README.md` extended guide links (de-duplicated)
- Updated: `docs/README.md` reading paths + index ordering
- Updated: `docs/guides/README.md` start path + supplemental placement

## Follow-up Rules

When adding future guides:
1. Add to `docs/README.md` guide index.
2. Add to `docs/guides/README.md` where appropriate.
3. Add to `README.md` only if newcomer-critical.
4. Use supplemental sections for legacy maps and non-primary navigation aids.

## Related Guides

- `029-docs-qa-checklist-and-validation.md`
- `030-contributor-onboarding-15-minutes.md`
- `031-docs-navigation-cleanup.md`
