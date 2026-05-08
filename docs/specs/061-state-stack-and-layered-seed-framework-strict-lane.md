# 061 - State Stack And Layered Seed Framework Strict Lane

Roadmap: [`g04.019`](../roadmaps/g04/019-state-stack-and-layered-seed-framework.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Purpose

Define the contract and first proof boundary for a standard Effigy state-stack
framework above the shipped artifact substrate.

This lane exists because Acowtancy has exposed the real missing piece: not OCI
transport, but the ordered lifecycle for structure, seed, imported data,
captures, and rebuilds.

## Hard Boundaries

- keep Effigy app-agnostic
- do not move repo-specific transform or conflict logic into Effigy
- keep `artifact kind` separate from `layer role`
- no automatic sync daemon or background replication
- no `.github/workflows/` edits
- no release execution

## Current Ready Card

None. The lane is complete for this release slice.

## Execution Chain

- `593` complete: opened the lane, promoted the initial roadmap and contract
  anchors, and selected the first contract-shaping card
- `594` complete: promoted the phase model, stack manifest, and Acowtancy proof
  boundary
- `595` complete: implemented state-stack manifest and lineage plan foundation
- `596` complete: added a plan-only state-stack command surface
- `597` complete: closed the foundation pass and selected JSON contract
  examples as the next boundary
- `598` complete: added state-stack JSON contract examples and command lookup
- `599` complete: added the first repo-native composed-manifest state config
  boundary
- `600` complete: added a durable operator-visible lineage report location before
  execution adapters
- `601` complete: added the first bounded execution adapter for task-mode layers
- `602` complete: added artifact staging to state-stack apply reports without
  applying payload semantics
- `603` complete: designed the SQL apply adapter boundary before executing SQL
  payload layers
- `604` complete: added the narrowest safe SQL import adapter through existing
  database seed/import plumbing, including target preflight before execution
- `605` complete: designed the state capture report boundary before capture
  execution
- `606` complete: added a plan-only `state capture` command surface
- `607` complete: added local artifact staging for an already-produced capture
  payload
- `608` complete: added explicit OCI publish for state capture artifacts
- `609` complete: added repo-owned capture task execution before artifact
  staging
- `610` complete: designed lineage-history lookup before adding more execution
  semantics
- `611` complete: added a read-only state history command over report files
- `612` complete: added latest and timestamped report history writes while
  preserving the legacy plan report path
- `613` complete: closed the first state-stack proof slice with documentation and
  validation
- `614` complete: hardened the capture task context contract with a versioned
  JSON context file and env alias
- `615` complete: ran the first Acowtancy-side state-stack proof and fixed
  routed workspace-container env forwarding for capture task context
- `616` complete: closed the state-stack release slice and held this as the next
  release boundary

## Exit Condition

This lane is closed for the next release boundary. Effigy now has a durable
contract for layered seed/migration state, a bounded Acowtancy proof loop, and
operator-visible plan/apply/capture/history surfaces that do not execute
app-specific semantics.

## Next Task

Hand off to the release-prep thread. Release execution remains human-owned.
