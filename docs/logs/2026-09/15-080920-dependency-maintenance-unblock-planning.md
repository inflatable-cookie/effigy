# Dependency Maintenance Unblock Planning

Status: complete
Created: 2026-09-15
Roadmap: g10.007
Batch: dependency-maintenance-unblock-planning

## Summary

Promoted one lockfile-bounded maintenance task to clear a base-wide cargo-deny
failure before g10.006 resumes.

## Changes

- Preserved g10.006, PR #109, its retained workspace/worker/reviewer, and its
  accepted exact-head review.
- Added ready task `g10.007` for patched rustls and non-yanked chacha20 lock
  resolution with no manifest or policy change.
- Made `g10.007` first in delivery order without mutating the already-dispatched
  g10.006 Queue dependency list.
- Added a committed ready-to-launch handoff for the existing coordinator.

## Vision Target Delta

- Primary tags: `CONTRACT`, `MAINT`, `RELEASE`
- Movement: base-wide supply-chain failure blocked an unrelated reviewed PR ->
  one isolated maintenance lane has explicit execution authority.
- Remaining gap: maintenance implementation/review/merge, then retained g10.006
  rebase, revalidation, review, merge, and closeout.

## Validation Performed

- clean `main` reproduced RUSTSEC-2026-0285 and the chacha20 yank warning
- targeted `cargo update --dry-run` selected patched/yank-free versions and
  three resolver-required transitive updates
- `git diff --check` and `effigy qa:docs` validate this planning batch before
  publication

## Risks

- A broader resolver delta, manifest requirement, or failing crypto/TLS consumer
  returns to Chatterbox rather than widening the lane.
- PR #109 must be rebased and re-reviewed after the maintenance merge.

## Next Task

Coordinator dispatches `g10.007`, then resumes the retained g10.006 Queue task
after maintenance closeout.
