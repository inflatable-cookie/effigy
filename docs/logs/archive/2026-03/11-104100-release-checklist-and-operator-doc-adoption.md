# 2026-03-11 10:41:00 - release checklist and operator doc adoption

## Summary
- Updated the release checklist template (`014`) to use the built-in
  `effigy release` workflow for simulation, readiness checks, preparation,
  execution preflight, execution, and tag-install verification.
- Updated repo-level maintainer/operator docs to point at the same built-in
  release flow, while keeping the shell scripts documented as backup channels
  for migration safety and external tooling.
- Marked roadmap section 8's release-checklist documentation task complete.

## Why
- The release system now has enough shipped coverage that the docs should stop
  steering maintainers toward script-first release operation.
- Keeping the wrapper backup policy explicit avoids overstating migration
  completion while still making the preferred operator path clear.

## Verification
- `git diff --check`
