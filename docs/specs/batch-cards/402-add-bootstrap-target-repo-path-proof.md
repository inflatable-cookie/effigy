# 402 - Add Bootstrap Target Repo Path Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Prove bootstrap task execution keeps the cloned target repo as path authority
instead of drifting to the invocation cwd.

## Scope

- add or tighten a focused bootstrap proof
- use a synthetic remote/target repo only
- assert bootstrap setup/start task execution writes into the target repo
- assert invocation cwd does not become the effective repo root for embedded
  task execution
- avoid live external repos or network

## Exit Condition

This card is complete when the bootstrap proof fails if target repo path
authority drifts back to invocation cwd.

## Next Task

Add the bootstrap target repo path proof.
