# 02 014 Typed Container Assembly Foundation

Date: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`
Batch: `336`

## What Landed

- added a first typed generated-compose model inside
  `crates/effigy-containers/src/policy_support.rs`
- moved the first high-value generated-compose seams onto that typed model:
  - shared-service env injection
  - generated port publication
- kept media and host-mount rewrites out of scope for this batch
- added unit proofs for:
  - sequence `environment` conversion into typed mapping state
  - typed port rewrite with non-string port entries preserved

## Why This Boundary

The main container brittleness issue is still broader than this batch, but
`336` now makes one real change in ownership:

- shared-service env and port policy no longer each parse the compose YAML
  string as their working model
- one typed generated-compose document now owns both seams together before
  writeout

That is enough to prove the lane is real and to make the next decision
explicit instead of speculative.

## Validation

- `cargo test -p effigy-containers policy_support::tests:: --lib -- --nocapture`
- `cargo test -p effigy-containers generated_compose_ --lib -- --nocapture`
