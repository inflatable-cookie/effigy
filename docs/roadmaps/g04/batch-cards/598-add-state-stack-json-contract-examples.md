# 598 - Add State Stack JSON Contract Examples

Lane: [`061-state-stack-and-layered-seed-framework-strict-lane.md`](../061-state-stack-and-layered-seed-framework-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-08

## Goal

Document and validate the first JSON contract examples for `effigy state plan`.

## Scope

- add representative JSON payload examples for state-stack lineage planning
- include an Acowtancy-shaped fixture in the examples or tests
- wire the examples into the existing docs/JSON validation path if that path
  supports the shape cleanly
- update guides or contract docs only where needed for discoverability

## Non-Goals

- no new command behavior
- no `apply`, `capture`, or `rebase` execution
- no app hook execution
- no durable lineage ledger
- no release work

## Exit Condition

This card is complete when the state-stack plan JSON shape is documented and
checked by the normal focused validation path.

## Validation

- PASS: `cargo run --bin effigy -- docs check-json-examples --file docs/guides/026-json-payload-examples.md`
- PASS: `cargo run --bin effigy -- docs check-paths docs/guides/017-json-output-contracts.md docs/guides/025-command-reference-matrix.md docs/guides/026-json-payload-examples.md docs/contracts/json-schema-index.json docs/contracts/fixtures/state-stack/acowtancy-uat.toml docs/roadmaps/g04/batch-cards/598-add-state-stack-json-contract-examples.md docs/roadmaps/g04/batch-cards/599-add-state-stack-manifest-config-boundary.md docs/specs/061-state-stack-and-layered-seed-framework-strict-lane.md docs/roadmaps/g04/019-state-stack-and-layered-seed-framework.md`
- PASS: `cargo run --bin effigy -- contracts check-json --fast --print-selected=text`
- PASS: `git diff --check`

## Next Task

Start
[`599-add-state-stack-manifest-config-boundary.md`](./599-add-state-stack-manifest-config-boundary.md).
