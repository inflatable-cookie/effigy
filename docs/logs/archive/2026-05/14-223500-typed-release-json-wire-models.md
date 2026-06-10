# Typed Release JSON Wire Models

Date: 2026-05-14
Roadmap: `g06.006`
Batch card: `806`

## Summary

Replaced the release command family's repeated ad hoc JSON payload assembly
with typed serializable wire models.

## Changes

- rewrote
  [`crates/effigy-release/src/render_json.rs`](/Users/tom/Dev/projects/effigy/crates/effigy-release/src/render_json.rs)
  around typed payload structs and shared pretty-rendering helpers
- typed these release wire families:
  - status
  - gates
  - verify-install
  - prepare plan
  - simulate
  - prepared
  - execute plan
  - resume
  - execute
- typed shared nested payloads for:
  - version source
  - changelog
  - unreleased counts
  - gate results
  - verification step results
  - mutation plans
  - source fingerprint drift
  - working-tree and resume drift projections

## Outcome

- `crates/effigy-release/src/render_json.rs` no longer contains `json!` builders
- the release JSON contract family now has one clear typed owner
- release wire-shape drift risk is lower because nested payload fields are
  shared across all release renderers

## Validation

- `cargo test json_contract`
- `cargo test deploy_provider`
- `cargo test release`
