# Docs QA Report Index Helper Validation

Date: 2026-03-02
Owner: Effigy
Related roadmap: shell completion and command discovery polish

## Scope
- Reduce report-index maintenance friction by adding a helper command for `docs/reports/README.md` updates.

## Changes
- Added `scripts/add-report-index-entry.sh`:
  - accepts a report filename or path
  - validates report exists under `docs/reports` and is a `.md` artifact
  - no-ops if already indexed
  - inserts the markdown link entry before archived report links
- Updated docs QA guide `029-docs-qa-checklist-and-validation.md` with helper usage.

## Validation
- command: `./scripts/add-report-index-entry.sh docs/reports/2026-03-02-docs-qa-report-index-helper-validation.md`
  - result: pass (entry inserted)
- command: `./scripts/check-doc-reports-index.sh`
  - result: pass
- command: `./scripts/check-quality-gates.sh --docs-only`
  - result: pass

## Risks / Follow-ups
- Helper currently inserts near the archived boundary instead of date-sorting inside the recent list.

## Next
- Add optional `--sort` mode to normalize report index ordering by filename date.
