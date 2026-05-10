# 654 - Collapse Routed Container Exec Variants

Roadmap: [`../026-shared-dispatcher-and-exec-collapse.md`](../026-shared-dispatcher-and-exec-collapse.md)
Strict lane: [`../../../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md`](../../../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md)
Contract: [`../../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md`](../../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Collapse the current near-duplicate routed container-exec variants behind one
shared internal path while keeping all caller-visible behavior unchanged.

## Scope

- unify run vs capture branching behind one shared implementation seam
- unify explicit-policy vs resolved-policy branching behind the same seam
- keep the current public helper names if thin wrappers are the safest landing
- preserve current routing, capture, and error behavior exactly

## Acceptance

- the routed container-exec duplication is materially reduced
- current callers keep the same surfaced behavior
- focused exec and embedded-task proofs stay green
