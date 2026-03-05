# Docs QA Reports Index Check Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Add docs QA automation that enforces report index completeness.

## Changes
- Added `scripts/check-doc-reports-index.sh`:
  - validates every `docs/logs/*.md` file is indexed in `docs/logs/README.md`
  - fails on stale index links that reference missing files
- Wired the checker into:
  - `scripts/check-quality-gates.sh --docs-only`
  - `scripts/check-prepush-ci.sh`
- Backfilled missing historical report links in `docs/logs/README.md` so the new gate starts from a clean baseline.
- Updated docs QA guide `029-docs-qa-checklist-and-validation.md` with checker behavior.

## Validation
- command: `./scripts/check-doc-reports-index.sh`
  - result: pass
- command: `./scripts/check-quality-gates.sh --docs-only`
  - result: pass

## Risks / Follow-ups
- Any report renames now require synchronized `docs/logs/README.md` updates or docs QA will fail.

## Next
- Add a small helper script to auto-append newly created report files into the reports README section to reduce manual index churn.
