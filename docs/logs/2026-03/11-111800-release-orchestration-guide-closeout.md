# 2026-03-11 11:18:00 - release orchestration guide closeout

## Summary
- Added guide `051-release-orchestration.md` as the dedicated reference for the
  shipped `effigy release` and `effigy changelog extract` surfaces.
- Updated the docs hub, command matrix, and `CLAUDE.md` so maintainers and
  agents can find one canonical release-orchestration guide.
- Reconciled roadmap section 10 to the code/docs that already ship, including
  the existing comprehensive `effigy release --help` surface.

## Why
- Section 10 still described the release documentation as fragmented and partly
  missing even though most of the command surface was already implemented.
- This batch makes the release feature discoverable from stable docs instead of
  forcing users to piece it together from the roadmap, help text, and logs.

## Verification
- `git diff --check`
