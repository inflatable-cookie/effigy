# g06.006 - Typed Contract Shape Reuse And JSON Builder Reduction

Status: Complete
Depends on: `g06.001`

## Goal

Reduce repeated dynamic JSON-shape assembly by promoting typed reusable
contract models where the same payload shape is built in multiple places.

## Evidence

- deploy/provider work recently benefited from typed context/report models
- repeated `serde_json::json!` payload construction tends to sprawl, drift, and
  duplicate validation logic
- Effigy carries many machine-readable surfaces where plain Rust structs are
  cheaper to maintain than ad hoc JSON assembly

## Scope

- inventory repeated JSON-like shapes across release, demo, deploy, scan, and
  adjacent command families
- promote typed serializable structs where two or more builders shape the same
  payload family
- converge validation and projection helpers where that reduces drift
- keep wire shapes unchanged unless a separate contract opens a change

## Out Of Scope

- no blanket ban on `json!`
- no forced typing for one-off payloads
- no cross-crate contract rewrite without evidence
- no breaking JSON contract changes

## Guardrails For A Cheaper Model

- only type shapes that are reused or validation-heavy
- keep serialization code boring and explicit
- do not leak internal Rust-only concepts into public wire models
- preserve field names, ordering expectations, and schema versions exactly

## Suggested Implementation Steps

1. Inventory repeated payload families.
2. Pick the highest-drift surfaces first.
3. Introduce typed models and golden tests.
4. Replace repeated builders incrementally.
5. Leave one-off payloads alone.

## Acceptance Criteria

- repeated machine-readable shapes are more centralized and typed
- JSON contract drift risk is reduced
- payload tests become clearer or cheaper to maintain
- no public wire shape regresses

## Validation

Minimum focused validation:

```bash
cargo test json_contract
cargo test deploy_provider
cargo test release
```

## Current State

The first typed wire-shape reuse slice is landed:

- the full release JSON contract family now renders through typed payload
  structs
- shared nested release payloads now have one owner instead of repeated
  hand-built maps
- release JSON output behavior stayed stable under contract and CLI coverage

Other payload-heavy areas still exist, but the next stronger lean-down target
is the compatibility-branch audit rather than expanding this slice into every
JSON surface at once.

## Next Task

Continue with `g06.007`.
