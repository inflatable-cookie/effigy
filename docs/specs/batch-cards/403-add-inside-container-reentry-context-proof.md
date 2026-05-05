# 403 - Add Inside Container Reentry Context Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Prove inside-container task re-entry keeps the captured runtime context stable.

## Scope

- add or tighten a focused runtime/execution proof
- simulate an inside-container handoff context
- assert container-targeted execution resolves locally when already inside the
  container handoff
- assert repo/cwd path authority comes from the captured context, not fresh env
  probing

## Exit Condition

This card is complete when the proof fails if inside-container re-entry starts
guessing host/container state or path authority again.

## Next Task

Add the inside-container re-entry context proof.
