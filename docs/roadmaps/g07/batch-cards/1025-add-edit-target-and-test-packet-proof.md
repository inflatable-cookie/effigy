# 1025 - Add Edit Target And Test Packet Proof

Roadmap: [`../075-edit-target-and-related-test-packets.md`](../075-edit-target-and-related-test-packets.md)
Strict lane: [`../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md`](../../../specs/096-graph-agent-adoption-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-20

## Purpose

Make graph packets more directly useful before editing.

## Work

- identify current packet fields that agents use after `graph explore`
- add or refine likely edit-target and likely test-target projections
- use confidence labels for inferred relationships
- add fixture-backed proof for split ownership
- update JSON contract docs when payloads change

## Guardrails

- no exhaustive-test claims
- no oversized packets
- no removal of lower-level graph commands
- no language support claims beyond current extractors

## Acceptance

- split-feature packets separate implementation, wiring, and tests where
  evidence supports it
- low-confidence cases are labeled honestly
- JSON tests pin changed payloads

## Next Task

Move to `1026`.
