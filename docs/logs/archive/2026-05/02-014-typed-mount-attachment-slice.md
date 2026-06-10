# 02 014 Typed Mount Attachment Slice

Date: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`
Batch: `338`

## What Landed

- extended the typed generated-compose model with typed `volumes`
- moved generated media mount attachment onto that typed model
- moved generated host mount attachment onto that typed model
- moved repo-root-attached service detection for those paths onto the typed
  service owner instead of rediscovering it from raw YAML
- kept existing generated-compose behavior intact for:
  - duplicate mount suppression
  - repo-root-only attachment
  - current no-eligible-service failure shape

## Validation

- `cargo test -p effigy-containers policy_support::tests:: --lib -- --nocapture`
- `cargo test -p effigy-containers generated_compose_policy_includes_declared_ --lib -- --nocapture`
- `cargo test -p effigy-containers generated_compose_ --lib -- --nocapture`
