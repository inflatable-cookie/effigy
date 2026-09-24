# g10.013 Closeout Currentness Repair

Status: complete
Created: 2026-09-24
Roadmap: g10.013
Batch: lifecycle-closeout-currentness

## Summary

- PR #114 merged at `fd198a260302743535e0612e55d6d7c324c41dbc`, but the Queue lifecycle hook held on two stale `Next Task` pointers.
- Removed the merged task from the active frontier and next-action prose. g10.012 remains the only approved repair in Queue.
- Kept the task card's Status-marker convergence and lifecycle projection with the hook.

## Vision Target Delta

- Primary tags: `CONTRACT`, `RELEASE`, `MAINT`.
- Movement: merged task still named as live frontier -> prospective currentness check clean.
- Remaining gap: Queue closeout retry, g10.012 reviewed closeout, then 0.13.0 release gates.

## Validation Performed

- Shared `auditCurrentnessText` with a synthetic terminal g10.013 record and planned hook-owned Status-marker removal: zero violations.
- `effigy qa:docs`: passed, including links, JSON examples, indexes,
  headings, forbidden text, workflow paths, and vision next-action checks.
- `git diff --check`: passed.

## Next Task

Retry the recorded Queue closeout hook against the repaired integration base. Continue g10.012 through Queue.
