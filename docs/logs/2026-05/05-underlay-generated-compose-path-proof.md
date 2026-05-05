# Underlay Generated Compose Path Proof

Date: 2026-05-05

## Summary

Completed card `401`.

## Outcome

Added a synthetic Underlay generated-compose proof that keeps runtime compose
output under `.effigy/runtime/compose`, mounts the target repo at
`/workspace-root/underlay-reference`, and preserves an external sibling mount at
`/workspace-root/underlay`.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers generated_compose_underlay_shape -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next

Card `402` adds the bootstrap target repo path proof.
