---
title: Papercuts wave 13 rhai env-race worker handoff
kind: northstar-handoff
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
handoff: single-file-path-only
status: ready-to-launch
owner: Tom / papercuts orchestrator
created: 2026-08-29
updated: 2026-08-29
handoff_path: /Users/tom/Dev/projects/effigy/docs/handoffs/20260829-231900-papercuts-wave13-rhai-env.md
base_required: pushed-main
tags: [coordination, handoff, worker, pr, papercuts]
---

## What This Thread Was Doing

Wave 12 closed committed path/`file:` status reporting. `cargo test
--workspace` still flakes two `effigy-rhai` runtime-context tests:
`execute_rhai_script_exposes_state_capture_context_helpers` and
`..._state_capture_set_in_capture_hook_context`. Each passes in isolation.
The scoped-env helper uses process-wide `set_var` / `remove_var` on
`EFFIGY_STATE_CAPTURE_CONTEXT`.

You are the Effigy implementation worker. Isolate that env so parallel
tests cannot clear each other's context.

## Why It Matters

Unrelated PRs keep paying a red `cargo test --workspace` on a race that
is not their change.

## Current State

- **Repository:** `/Users/tom/Dev/projects/effigy`
- **Planning branch:** `main`
- **Planning base commit:** `2b1aedd8bdb7ef27d76bd9efe578da9b026d57e3`
- **Pushed main verification:** local `HEAD` and `origin/main` both resolved
  to that SHA before this handoff was created.
- **Planning checkout:** clean before this handoff file was created.
- **Worker mode:** implementation worker dispatched by the orchestrator.
- **Worker branch:** `worker/papercuts-wave13-rhai-env`
- **Worker worktree:** launcher first.
  `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`.
- **Required sibling worktree links:** none
- **Ready work items, in order:**
  1. `effigy-rhai` runtime-context tests race on process-wide env vars —
     stop sharing `std::env::set_var` for `EFFIGY_STATE_CAPTURE_CONTEXT`
     across concurrent tests. Mutex, thread-local, or a non-global
     inject seam are all in-bounds if they keep the runtime contract
- **Out of scope:** deps status/link behaviour; GitHub workflows;
  release mutations.
- **Canonical refs:** `PAPERCUTS.md`;
  `crates/effigy-rhai/src/tests/mod.rs` scoped-env helper;
  `crates/effigy-rhai/src/tests/runtime.rs`.
- **Required validation:** `cargo test -p effigy-rhai` and a
  `cargo test --workspace` pass (or the two named tests under
  `--test-threads` stress). Close the papercut. Do not run
  `release prepare/execute`.
- **PR URL:** pending
- **Merge authorisation:** absent; do not merge

## Boundaries

- Test isolation only. Do not change capture semantics for production
  callers.
- Do not merge.

## Important Context

- Both tests set the same process env; one teardown races the other.
- **Report to:** the operator.

## Suggested Next Move

Read this file from the top. Run the worktree-safety preflight. After
the committed `HEAD` handoff checks out, fix the scoped-env helper.

## Completion Protocol

### Before you start

1. Read this handoff path. Its `worker_mode: implementation` and
   `dispatch_authority: orchestrator` metadata activate worker mode. Then
   run `git rev-parse --show-toplevel`, `git branch --show-current`,
   `git status --porcelain`, and `git worktree list --porcelain`.
2. If the current root is a registered worktree, status is empty, and the
   branch is not `main`, accept it. Record the actual path/branch.
3. If the launcher supplied a dirty or `main` worktree, stop and report
   it. Fallback container is
   `AGENTS_WORKTREE_CONTAINER_DIR=/Users/tom/Dev/worktrees`. Never use
   `/tmp`.
4. From the selected worktree, record the repository-relative path
   `docs/handoffs/20260829-231900-papercuts-wave13-rhai-env.md`.
   Confirm `HEAD == origin/main`, ancestor
   `2b1aedd8bdb7ef27d76bd9efe578da9b026d57e3`, and that relative path in
   `HEAD`. Load
   `git show HEAD:docs/handoffs/20260829-231900-papercuts-wave13-rhai-env.md`.
   If the absolute dispatch file differs, stop. The `HEAD` copy is
   canonical.
5. Required sibling list is `none`. Skip link setup.
6. Read `AGENTS.md` and `PAPERCUTS.md`.

### When the assigned runway is complete

1. Close/add-and-close the item in this repo's `PAPERCUTS.md`.
2. Push a PR. Do not merge.

### Review and merge path

Awaiting orchestrator review. Merge is operator-authorised only.

- **Closeout refs:** `PAPERCUTS.md`; this handoff; the PR.

### Handoff closeout

If the race is already gone on this SHA, close with evidence.
