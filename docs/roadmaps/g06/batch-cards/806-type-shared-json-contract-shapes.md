# 806 - Type Shared JSON Contract Shapes

Roadmap: [`../006-typed-contract-shape-reuse-and-json-builder-reduction.md`](../006-typed-contract-shape-reuse-and-json-builder-reduction.md)
Strict lane: [`../../../specs/084-codebase-lean-down-strict-lane.md`](../../../specs/084-codebase-lean-down-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Delete repeated dynamic JSON-shape assembly where the same contract family is
built or validated in more than one place.

## Scope

- inventory reused JSON payload families
- promote typed serializable models for the highest-drift families
- keep one-off payloads dynamic if typing them adds no real payoff

## Acceptance

- repeated machine-readable shapes have clearer shared owners
- drift-heavy `json!` builders are reduced
- JSON contract behavior stays unchanged

## Completed

- Replaced the release command family's repeated JSON payload assembly with
  typed wire payload structs in
  [`crates/effigy-release/src/render_json.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-release/src/render_json.rs).
- Typed the shared nested payloads reused across release status, simulate,
  prepare, execute, resume, gate, and verify-install surfaces.
- Removed all `json!` builders from the release JSON render module.
- Logged the slice in
  [`../../../logs/archive/2026-05/14-223500-typed-release-json-wire-models.md`](../../../logs/archive/2026-05/14-223500-typed-release-json-wire-models.md).

## Suggested Validation

```bash
cargo test json_contract
cargo test deploy_provider
cargo test release
```

## Next Task

Execute `807`.
