# Closed Generation Lifecycle Normalization

Status: complete
Created: 2026-09-08
Roadmap: none — `g09` closed
Batch: closed-generation-lifecycle-normalization

## Summary

- Archived the closed `2026-09` log window and removed it from the active log
  index.
- Deleted the completed g09 worker handoff after Northstar Queue commit
  `fd3c2c82c525b2ffe3fb20e6d6de6e77d452553d` restored the intended verifier
  rule: exactly the submitted handoff may be deleted, while other closeout
  Markdown must remain regular files.
- Preserved expanded roadmap history under Effigy's explicit repo-local
  retention convention; front doors, not closed generation directories, are
  the compact session surface.
- Removed the resolved lifecycle triage note and pointed the planning spine at
  operator-led Northstar Atlas discovery. No g10 lane was opened.

## Preservation Manifest

- Source log tree: `docs/logs/2026-09/` at planning commit
  `42b956a0f385e6b92105e465f6cf56cec0bd7c8a`, tree
  `91d586f466b06bd4058971bd3dfd01c161b5628e`, 34 tracked Markdown files.
- Destination: `docs/logs/archive/2026-09/`, preserving every source file plus
  this normalization record.
- Link rewrite: every tracked Markdown reference containing
  `logs/2026-09/` now names `logs/archive/2026-09/`.
- Exact deletions: the submitted closed handoff
  `docs/handoffs/20260908-142948-stale-local-install-recovery.md` and the fully
  resolved triage note
  `docs/triage/20260908-161956-closed-generation-lifecycle-drift.md`.
- Unique authority removed: none. Contract `001`, the roadmap retention
  convention, queue history, and Git retain the governing rules and evidence.
- Open commitments rehomed: none. The three remaining triage notes stay open
  with current owners and next checks.

## Classification

- `g01` through `g09`: safely closed. Their expanded roadmap directories remain
  legitimate project history under the repo-local retention disposition.
- `docs/logs/2026-09/`: safely closed and archived.
- g09 worker handoff: completed dispatch overlay, safe to delete after the
  queue verifier correction.
- next generation: not selected; no active or ready execution lane exists.

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`, `CONTRACT`
- Movement: closed generation with an active log window and retained dispatch
  overlay -> compact closed evidence with no false execution authority
- Remaining gap: next strategic runway is operator-owned

## Validation Performed

- `effigy qa:docs`
  - passed after correcting one moved log's archive-depth link to
    `PAPERCUTS.md`
- `git diff --check`
  - passed

## Risks

- Historical roadmap bodies remain expanded by explicit policy. They are not
  active authority and current front doors do not enumerate them as a queue.
- The queue plugin must be reloaded before a future task relies on commit
  `fd3c2c8`; this normalization does not mutate or reload the plugin.

## Next Task

Use Northstar Atlas with the operator to choose the next strategic runway. Do
not open `g10` or authorize execution from this archived log.
