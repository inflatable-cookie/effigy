---
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
lane: cargo-version-transition-link
base_commit: 729aa89ccd48e98eaf77aa5b8716cd835c774823
branch: fix/cargo-version-transition-link
workspace: /Users/tom/.paseo/worktrees/310mya31/effigy-cargo-version-link
---

# Worker Handoff: Cargo Version-Transition Linking

Implement the operator-authorized bounded fix in the existing workspace and
branch. The Swallowtail v0.4.4 Desktop pre-release reproduces this failure:
`effigy deps link cargo` from Git-pinned v0.4.3 to local candidate packages
v0.4.4 installs patches but does not refresh affected locked Git packages
before metadata verification. Cargo records eight `[[patch.unused]]` entries,
verification fails, Effigy rolls back config, but `Cargo.lock` residue remains.

## Required outcome

Refresh only affected linked packages across the version transition before
metadata verification. Include every affected lockfile in the transactional
rollback so failed verification restores the exact baseline. Add a real
v0.4.3-to-v0.4.4 fixture and a failed-verification rollback regression.
Preserve unrelated lockfiles and existing link behavior. Do not change
Desktop pins or Swallowtail manifests.

## Scope boundary

Use the existing `deps link cargo` transaction and package/link model. No
global `cargo update`, broad lockfile rewrite, release mutation, workflow
change, or unrelated dependency behavior. Stop and return facts if the fix
requires widening beyond affected packages or the existing transaction.

## Papercut reconciliation

The primary checkout at `/Users/tom/Dev/projects/effigy` has an uncommitted
operator entry at the top of `PAPERCUTS.md` titled:
`Cargo link cannot cross a local package version bump and leaves lock residue`
(dated 2026-09-07). Reconcile that exact entry into this branch, preserving
all unrelated primary-checkout changes; do not reset or overwrite the primary
checkout. Mark it resolved with the shipped fix and evidence once the lane is
complete, keeping the existing writing style.

## Validation and handoff

Run focused deps-link/Cargo tests, the real v0.4.3 -> v0.4.4 fixture and
failed-verification rollback regression, relevant `effigy qa` or targeted QA,
fmt, clippy with `-D warnings`, and `git diff --check` as appropriate. Open a
reviewable PR from this branch, report the exact head and clean worktree, and
do not merge it yourself. The coordinator owns independent review, merge,
closeout, and the accepted binary/source SHA.

## Next Task

Open the PR at the exact validated head; the coordinator will launch an
independent cross-model review and enforce the merge gate.
