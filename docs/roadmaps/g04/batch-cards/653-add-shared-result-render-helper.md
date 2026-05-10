# 653 - Add Shared Result Render Helper

Roadmap: [`../026-shared-dispatcher-and-exec-collapse.md`](../026-shared-dispatcher-and-exec-collapse.md)
Strict lane: [`../../../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md`](../../../specs/069-shared-dispatcher-and-exec-collapse-strict-lane.md)
Contract: [`../../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md`](../../../contracts/024-shared-dispatcher-and-exec-collapse-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-10

## Purpose

Land the first real code slice for `026`: a shared result-render helper for
command owners that already carry both JSON and text payloads.

## Scope

- add one shared helper for existing success/failure json+text rendering
- apply it to the lowest-risk command owners first
- keep all schema ids, text, and error meaning unchanged
- leave exec collapse and release-stage reuse for later cards

## Acceptance

- one shared render seam exists
- at least the selected low-risk command owners use it
- focused parser/runner/output proofs stay green
