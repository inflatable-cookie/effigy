# 017 - Architecture Map And Authority Surface Repair

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 013, 014, 015, 016

## Problem

Some of Effigy's architecture authority surfaces no longer describe the real
 codebase shape.

In a docs-driven repo, stale package and ownership maps are not harmless.
 They push future planning and refactors toward fake boundaries.

## Goal

Repair the architecture and authority surfaces so they describe the
 post-hardening runtime/container structure truthfully.

## Scope

- rewrite or replace stale architecture maps that no longer match the code
- make current module ownership explicit for:
  - runner orchestration
  - activation/session context
  - container assembly
  - workspace handoff
  - runtime/container error families
- prefer smaller accurate authority docs over broad drifting inventories
- remove or shrink architecture artifacts that cannot realistically be kept
  current

## Non-Goals

- reopening finished execution work just to make docs prettier
- introducing new product behavior
- opening a new roadmap generation

## Exit Condition

This milestone is complete when:

- the architecture front doors match the actual current runtime/container
  structure
- no major live subsystem is primarily described by stale historical docs
- roadmap and spec work can use the repaired architecture surfaces as real
  authority again

## Next Task

Hand off to `g03.018` and prove the hardened runtime/container core with
 executable stress scenarios.
