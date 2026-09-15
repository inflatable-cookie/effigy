---
kind: northstar-handoff
title: "g10.007 — Refresh vulnerable and yanked Cargo lock entries"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
owner: Tom
created: 2026-09-15
updated: 2026-09-15
base_required: pushed-main
roadmap: docs/roadmaps/g10/007-refresh-vulnerable-and-yanked-cargo-lock.md
queue_dispatch: northstar-queue
queue_approval: "Operator-confirmed direction on 2026-09-15 authorized a separate dependency-maintenance lane to unblock Queue task 1cddcbfc-7589-4bea-bf9d-79582f7bb448 and explicitly prohibited widening g10.006."
queue:
  capability: general
  skipPRReview: false
  notifyOriginOnCloseout: false
---

## What This Thread Was Doing

Chatterbox resolved a g10.006 verification blocker caused by Effigy's clean main
dependency state, not by PR #109. The operator authorized a separate maintenance
lane and required the accepted graph implementation/review to remain intact.

## Why It Matters

The required supply-chain check rejects `rustls 0.23.43` for
RUSTSEC-2026-0285 and warns that `chacha20 0.10.1` is yanked. PR #109 changes no
Cargo manifest, lockfile, or deny policy and cannot pass the base-wide gate until
main is repaired.

## Current State

- Canonical task: [`g10.007`](../roadmaps/g10/007-refresh-vulnerable-and-yanked-cargo-lock.md).
- Clean main/planning base reproduced both cargo-deny findings.
- A targeted Cargo dry-run selects `rustls 0.23.45`, `chacha20 0.10.2`, and only
  compatible resolver-required `aws-lc-rs`, `aws-lc-sys`, and
  `rustls-webpki` updates.
- Queue task `1cddcbfc-7589-4bea-bf9d-79582f7bb448`, worker
  `ca06a0cb-7ed3-4c97-ada4-454b6d9df163`, reviewer
  `365f7580-6d00-43ac-a912-37b7949a55ff`, workspace
  `wks_e28f1bb13ac78f71`, PR #109, and reviewed head
  `5872b72377f252a8ae5586dd22a7f2bfc651e634` are preserved.

## Boundaries

Update committed lock resolution in a separate PR. Own only `Cargo.lock`, a
bounded changelog entry, and task evidence. Do not edit manifests, dependency
requirements/features, `deny.toml`, workflows, release surfaces, source code,
PR #109, or g10.006. Do not add an exception for the live rustls vulnerability
or perform opportunistic upgrades.

## Important Context

The exact targeted command shape proven in dry-run is:

```sh
cargo update -p rustls@0.23.43 --precise 0.23.45 -p chacha20@0.10.1
```

Cargo also moves `aws-lc-rs 1.17.3 -> 1.18.1`, `aws-lc-sys 0.43.0 -> 0.45.0`,
and `rustls-webpki 0.103.13 -> 0.103.15`. Any broader delta requires inspection
and, if not strictly resolver-required, a stop.

### UI Design Brief

Not applicable.

## Suggested Next Move

Reproduce the clean-base gate once, apply the targeted update, and inspect the
complete `Cargo.lock` diff before running tests. Prove both old versions are
absent and run full `cargo deny check`, not only advisories.

## Completion Protocol

Open one non-draft Queue-managed PR separate from PR #109. Run the task's cargo
tree, full cargo-deny, focused gateway/secrets/Rhai, workspace, fmt, clippy,
docs, and diff validation. Independent exact-head review must confirm the
targeted lock-only boundary and both finding removals. After merge and hook
closeout, coordinator `5a87dde3-065d-4a2c-9811-c59fb020f077` resumes the
existing g10.006 Queue task/retained worker to rebase and revalidate PR #109;
do not mutate that task's frozen dependency list.
