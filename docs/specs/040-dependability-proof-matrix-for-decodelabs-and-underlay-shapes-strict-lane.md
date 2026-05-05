# 040 - Dependability Proof Matrix For DecodeLabs And Underlay Shapes Strict Lane

Roadmap: [`g03.034`](../roadmaps/g03/034-dependability-proof-matrix-for-decodelabs-and-underlay-shapes.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Purpose

Prove the new runtime context, container manager, and task execution request
surfaces against representative DecodeLabs and Underlay shapes without touching
real project repos or production data.

## Hard Boundaries

- do not mutate external DecodeLabs or Underlay checkouts
- do not use live production data
- do not edit `.github/workflows/`
- do not initiate release commands
- keep proof fixtures small and synthetic
- prefer focused tests over full-suite validation during fixture buildout

## Proof Areas

- DecodeLabs bundle-style DB seed path handling
- Rhai `exec::run(...)` container-targeted mysql import with `stdin_file`
- Underlay generated compose path handling
- bootstrap target repo path stability
- inside-container re-entry context stability
- host path and external mount mapping
- manager-backed operation reports
- direct task, bootstrap task, and Rhai task parity

## Current Ready Card

None. This lane is complete.

## Exit Condition

This lane closes when focused fixtures prove the DecodeLabs and Underlay shapes
that previously exposed runtime/path brittleness, and the next contract cleanup
milestone has an explicit start point.

## Next Task

Start `g03.035`.
