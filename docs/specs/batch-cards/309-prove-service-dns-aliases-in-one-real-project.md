# 309 Prove Service DNS Aliases In One Real Project

Status: next
Updated: 2026-04-22
Roadmap: `g02.020`
Spec: `docs/specs/020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`

## Objective

Land the next bounded `g02.020` slice by proving the shipped HTTP and TCP
service DNS model in one real consumer repo instead of stopping at library and
runner tests inside Effigy alone.

## In Scope

- migrate one real consumer repo onto the shipped `.test` alias model where it
  still hardcodes local service ports
- prove project-owned and shared-service aliases on the actual product path
- capture any bounded proof-exposed fixes needed to keep the shipped route and
  DNS model honest
- refresh lane-facing docs once the proof is trustworthy

## Out Of Scope

- broad migration across every consumer repo
- new alias categories or manifest-surface widening beyond proof-exposed fixes
- Linux or Windows resolver work
- unrelated local-network redesign

## Acceptance Criteria

- one real consumer repo can use shipped `.test` HTTP and TCP service names
  without depending on hardcoded local service ports
- the proof exercises both project-owned and shared-service alias behavior on
  the intended product path
- any proof-exposed fix stays bounded to making the shipped contract honest
- docs leave the lane with a truthful next continuation after the proof

## Validation

- proof commands and repo-local validation captured in the batch result
- `git diff --check`

## Result

Next. `308` finished the bounded shared-service reuse substrate, so the next
honest move is a real-project proof instead of more isolated internal
refinement.

## Next Task

Execute this card in the first consumer repo on the lane continuation chain:
`/Users/tom/Dev/projects/underlay-reference`.
