# 401 - Add Underlay Generated Compose Path Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Prove Underlay generated-compose path handling against the new runtime/container
foundation.

## Scope

- add or tighten a focused `effigy-containers` fixture for an Underlay-like
  generated compose shape
- assert generated compose lives under `.effigy/runtime/compose`
- assert workspace/root paths stay repo-targeted
- assert external sibling mount mapping remains stable
- keep the fixture synthetic and local to tests

## Exit Condition

This card is complete when the Underlay fixture fails if generated compose path
ownership or external mount mapping drifts.

## Closeout

Added a synthetic Underlay generated-compose proof in
`crates/effigy-containers/src/tests/compose.rs`.

The proof models an Underlay bundle-style repo with a generated PHP/nginx
container stack and an external sibling `underlay` mount.

It asserts:

- generated compose output stays under `.effigy/runtime/compose`
- the generated runtime compose path exists
- the project name stays stable for the Underlay repo/container shape
- the target repo is mounted at `/workspace-root/underlay-reference`
- the external sibling is mounted at `/workspace-root/underlay`

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers generated_compose_underlay_shape -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-containers -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Add the bootstrap target repo path proof.
