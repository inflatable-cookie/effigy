# 024 - Git History Cleanup

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-03
Depends on: —

## Problem

`.cache/cargo/` was committed to history and later removed, but the blobs remain
in the repository object database. Cargo registry artifacts as large as 1.4 MB
(unicode-width tables) and 814 KB (libc crate) still bloat every clone.

Additionally, roughly 2,200 checkpoint commits (`t3 checkpoint`) clutter the log
and inflate the object database with automated snapshots.

## Goal

Reduce repository clone size and log noise by cleaning up historical artifacts.

## Scope

- purge `.cache/cargo/` blobs from git history using `git filter-repo` or
  equivalent
- document the cleanup command and any post-cleanup steps (force-push, re-clone)
- evaluate whether checkpoint refs can be pruned or moved to a separate namespace
- measure clone size before and after

## Non-Goals

- rewriting active branch history that would break open PRs
- removing legitimate source history
- automating checkpoint cleanup unless the team requests it

## Exit Condition

This milestone is complete when:

- `.cache/cargo/` blobs are no longer present in the object database
- the documented clone size is measurably smaller
- there is a written record of what was done and how to avoid re-committing
  cache artifacts

## Next Task

If this lane is promoted, start by measuring current clone size and identifying
the exact commit range that introduced `.cache/cargo/`.
