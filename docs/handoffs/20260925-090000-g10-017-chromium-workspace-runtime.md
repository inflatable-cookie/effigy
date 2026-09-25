---
kind: northstar-handoff
title: "Effigy g10.017 — Opt-in Chromium workspace runtime"
handoff_mode: worker-pr-loop
worker_mode: implementation
dispatch_authority: orchestrator
status: ready-to-launch
base_required: pushed-main
queue_dispatch: northstar-queue
queue_approval: "Tom approved the two-part opt-in Playwright/Chromium workspace-runtime plan with 'Go for it' on 2026-09-25. This submission covers the Effigy core catalog task only."
queue:
  capability: general
  notifyOriginOnCloseout: true
---

## What This Thread Was Doing

Implement [g10.017](../roadmaps/g10/017-opt-in-chromium-workspace-runtime.md): the default-off Chromium system-library option in Effigy's built-in `workspace-rust-bun` service. The task card owns scope and acceptance.

## Why It Matters

Acowtancy g05.195 cannot complete independent browser review because its non-root linux-arm64 workspace container downloads Playwright's Chromium but lacks shared libraries to launch it. This is shared image functionality, not an app-specific Dockerfile override.

## Current State

- Start from pushed `main` after `g10.016` terminal closeout. Queue creates an isolated worker workspace and independent review.
- The [planning log](../logs/2026-09/25-090000-browser-runtime-frontier.md) records Tom's approval and the separate downstream bundle repository.
- The external `underlay-effigy-bundle` currently selects this catalog service. It does not yet expose a browser-runtime input and must not be edited by this worker.
- Chatterbox owns any product or version-gate decision. Queue owns worker, review, merge, and lifecycle closeout.

## Boundaries

Follow the task card's owned paths and proof. Keep `browser_runtime` default `none`; `chromium` adds only root-installed Debian runtime libraries and fonts. Do not bake browser binaries, Playwright, Node, or `npx` into the image. Do not edit the bundle, Acowtancy, workflows, tags, or release surfaces.

## Important Context

Read `AGENTS.md`, `docs/contracts/001-working-rules.md`, the task card, catalog service files, and `crates/effigy-catalog/tests/integration/workspace.rs`. The Acowtancy worker log on PR #345 names missing libraries and Playwright 1.55.1 revision 1193. Its user is `dev` on Colima aarch64. A static package list or `ldd` alone is not acceptance; the real arm64 non-root launch is required. Record solvable friction in `PAPERCUTS.md`.

## Suggested Next Move

Add the catalog parameter, Compose build arg, and explicit Dockerfile branch. Run focused assembly tests, then build and smoke the opted-in linux-arm64 image with Playwright 1.55.1 installed into `dev`'s cache. Record command/output and normal default-off behavior before opening a non-draft PR.

## Completion Protocol

Obtain independent exact-head review and green current-base CI before Queue merges. Record package list, arm64 build, non-root browser launch, validation, reviewed head, and merge in task evidence. Let the lifecycle hook close the task and remove this handoff. Notify the origin Chatterbox on closeout so it can advance the separate bundle and version-gate work.
