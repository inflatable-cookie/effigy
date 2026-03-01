# Reports

Reports capture execution evidence, checkpoints, and sweeps.

## Naming convention

Use date-first filenames:

- `YYYY-MM-DD-topic.md`
- `YYYY-MM-DD-HHMM-topic.md` (when multiple same-day reports are needed)

Examples:
- `2026-02-26-effigy-extraction-checkpoint.md`
- `2026-02-26-1545-path-install-smoke.md`

## Thread reports

When a feature spans multiple same-day checkpoints, add a consolidation report that links those checkpoints and provides one final validation matrix.

## Recent Release Notes

- [`2026-02-28-dag-watch-onboarding-release-note.md`](./2026-02-28-dag-watch-onboarding-release-note.md)
- [`2026-02-28-json-envelope-removal-release-note.md`](./2026-02-28-json-envelope-removal-release-note.md)
- [`2026-02-28-doctor-explain-mode-release-note.md`](./2026-02-28-doctor-explain-mode-release-note.md)

## Report template

```md
# <Report Title>

Date: YYYY-MM-DD
Owner: <team/person>
Related roadmap: <id/title>

## Scope
- ...

## Changes
- ...

## Validation
- command: `...`
  - result: ...

## Risks / Follow-ups
- ...

## Next
- ...
```
