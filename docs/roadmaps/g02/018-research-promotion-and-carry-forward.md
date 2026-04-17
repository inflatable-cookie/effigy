# 018 - Research Promotion and Carry-Forward

Generation: `g02`

Status: Planned
Owner: Research
Created: 2026-04-17
Depends on: 020, 021, 022

## Vision Alignment

Effigy's three research phases produced real dossiers, value tracks, and
translation memos. Since then, parts of that research have already been turned
into shipped product surface: env/schema work, manifest composition direction,
CLI/help improvements, release integration, gateway groundwork, and more.

What is missing now is not another broad research crawl. It is promotion and
carry-forward: reconcile the research corpus against the shipped codebase, then
keep only the genuinely unfinished future-facing questions visible.

## Primary Tags

- `RESEARCH`
- `MAINT`
- `CONTRACT`

## Target Envelope

- the finished `g01.020`–`g01.022` research corpus is promoted or indexed
  cleanly enough that those roadmaps can stay closed
- already-shipped conclusions are not left pretending to be open research
- the genuinely unfinished future-facing strands are isolated into one roadmap
- those strands are explicitly non-blocking for `v0.3`

## Vision Target Delta

- Move from `three half-open research roadmaps with mixed shipped and future
  concerns` toward `closed historical research phases plus one explicit
  carry-forward roadmap for the still-future questions`.

## Problem

The `g01` research phases are no longer a good live control surface:

- much of the research they asked for already exists
- some of the outcomes are already shipped in product work
- the remaining unchecked items are mostly promotion/index tasks or genuinely
  future-facing areas

That leaves the roadmap layer overstating what is still open and muddying the
release signal.

## Goals

- reconcile the `020`–`022` corpus against the current Effigy codebase
- promote stable conclusions into the right guides, architecture notes, and
  active roadmaps
- leave one explicit home for the remaining future-facing research debt
- keep non-`v0.3` research work visible without pretending it blocks release

## Non-Goals

- no broad new research crawl before `v0.3`
- no attempt to ship remote execution, IDE integration, plugins, or telemetry
  in this roadmap
- no reopening of closed `g01` phase files except to fix closeout state

## Carry-Forward Scope

### 1. Research Corpus Promotion

- update the research index and phase summaries
- identify which memos are now reflected in shipped product
- promote only the still-useful stable conclusions into maintained docs

### 2. DX Residue From Phase 2

- completion UX residue not already covered by the shipped CLI/help surface
- cross-platform portability follow-up that still matters after the current
  shell/runtime cleanup
- any still-useful DX pattern library material that should exist as a durable
  maintained artifact

### 3. Scale Residue From Phase 3

- remote execution strategy as a future-facing product question
- IDE/editor integration strategy
- plugin/extensibility posture
- telemetry/observability posture

These remain explicitly future-facing and non-blocking for `v0.3`.

## Exit Condition

This roadmap is complete when the closed `g01` research phases no longer carry
live ambiguity and the remaining future-facing research debt is explicit,
bounded, and clearly non-blocking for release.

## Next Task

Resume this roadmap only after the `v0.3` blocker is out of the way:

- `g02.010` remaining `/src` cleanup and reconciliation
