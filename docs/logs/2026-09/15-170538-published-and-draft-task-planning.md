# Published And Draft Task Planning

Status: complete
Created: 2026-09-15
Roadmap: g10.008
Batch: published-draft-task-promotion

## Summary

- reconciled completed g10.006 and g10.007 lifecycle truth with stale planning
  prose;
- defined `[tasks]` as the compatible published surface and `[drafts]` as an
  explicit lifecycle-labelled provisional surface;
- promoted one bounded implementation task and ready Queue handoff.

## Changes

- added architecture `028` and contract `046`;
- added ready task `g10.008` with mutable scope, adversarial oracle, validation,
  evidence, continuation, and stop conditions;
- published `g10.008` as the sole ready frontier and repaired active front-door
  pointers left behind after g10.006/g10.007 closeout;
- excluded local-only drafts, implicit discovery, generation, pruning, enforced
  expiry, access control, and automatic migration from v1.

## Vision Target Delta

- Primary tags: `ROUTE`, `OPERATE`, `MAINT`, `CONTRACT`
- Movement: one unbounded task inventory -> compatible published discovery plus
  an explicit provisional workbench using the same runtime
- Remaining gap: implementation and Queue-managed delivery of g10.008

## Validation Performed

- `effigy qa:docs`
- `git diff --check`
- semantic review of architecture, contract, task, handoff, and active pointers

## Risks

- Local ignored definitions remain deliberately unresolved until trust and
  precedence requirements come from a real consumer.
- Automatic cleanup remains unavailable; expiry supplies evidence, not deletion
  authority.

## Next Task

- Dispatch sole ready task `g10.008` through Northstar Queue.
