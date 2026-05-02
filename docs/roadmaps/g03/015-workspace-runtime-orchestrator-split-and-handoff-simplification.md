# 015 - Workspace Runtime Orchestrator Split And Handoff Simplification

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 005, 009, 010, 013, 014

## Problem

Workspace and runtime orchestration still live across several very large
 runner files with overlapping responsibilities.

Public workspace sessions, runtime prep, binary staging, cleanup ownership,
 and bootstrap handoff are too entangled. That makes the code hard to change
 confidently even when the intended behavior is clear.

## Goal

Split workspace/runtime orchestration into narrower owners and give public
 handoff plus cleanup policy one clear home.

## Current Boundary

The first split is now landed:

- public workspace entry
- bootstrap start handoff
- shared session ownership and cleanup resolution at the public shell boundary

The second split is now landed:

- workspace artifact and binary provisioning
- workspace permission and env preparation
- the glue between those prep steps and the public handoff path

The lane now closes cleanly. What remains in `workspace.rs` is mostly command
surface plus handoff/rendering glue, not another high-risk mixed ownership seam.
The next hardening priority is typed runtime/container errors.

## Scope

- break the current workspace/runtime hotspot into narrower modules for:
  - workspace session orchestration
  - workspace artifact and binary provisioning
  - workspace permission and env prep
  - handoff and cleanup ownership
- tighten boundaries between:
  - `container_runtime_prep`
  - workspace handoff
  - explicit `exec`
  - standard routed activation
  - deferral/runtime-prep reuse
- remove caller-local lifecycle branching where a shared orchestrator API can
  own the behavior
- leave one obvious owner for:
  - public workspace session creation
  - bootstrap start handoff
  - cleanup ownership resolution

## Non-Goals

- changing the user-facing shell UX
- adding new container/runtime features
- broad crate extraction beyond what this split directly needs

## Exit Condition

This milestone is complete when:

- workspace/runtime lifecycle logic is no longer concentrated in one mixed
  mega-file
- public runtime entrypoints delegate to narrower orchestration APIs
- ownership and cleanup rules can be traced through one clear module boundary

## Next Task

Closed. Promote `g03.016`.
