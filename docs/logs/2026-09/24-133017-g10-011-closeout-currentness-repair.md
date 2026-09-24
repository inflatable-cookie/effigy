# g10.011 Closeout Currentness Repair

Status: complete
Created: 2026-09-24
Roadmap: g10.011
Batch: lifecycle-closeout-currentness

## Summary

- PR #115 merged at `82d551c5f1f35e241061b0aba9d60ef52b20f7b7`, but the Queue lifecycle closeout held before writing a terminal record.
- Removed g10.011 from live frontier and Next Task prose while keeping g10.012 and g10.013 as the remaining approved repairs.
- Removed hand-maintained `Status:` headers from six older lifecycle-managed g10 task cards. Their lifecycle records and generated projection own current state; the old headers were duplicate authority, including stale `ready` claims.
- Left the g10.011 task card and generated lifecycle blocks to the closeout hook's own status convergence.

## Vision Target Delta

- Primary tags: `CONTRACT`, `RELEASE`, `MAINT`.
- Movement: closeout-blocking planning currentness drift -> clean read-only currentness audit.
- Remaining gap: Queue must retry and publish the g10.011 closeout, then g10.012 and g10.013 must finish before 0.13.0 release gates.

## Validation Performed

- `node /Users/tom/.agents/skills/northstar/scripts/lifecycle-core.ts audit-currentness --repo /Users/tom/Dev/projects/effigy`: passed, zero violations.
- `effigy qa:docs`: passed, including links, JSON examples, indexes, headings,
  forbidden text, workflow paths, and vision next-action checks.
- `git diff --check`: passed.

## Next Task

Retry the recorded g10.011 Queue closeout hook against the repaired integration base. Continue g10.012 and g10.013 through Queue.
