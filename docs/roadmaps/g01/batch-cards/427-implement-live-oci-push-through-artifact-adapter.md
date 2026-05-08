# 427 - Implement Live OCI Push Through Artifact Adapter

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06

## Goal

Add explicit live OCI push for staged artifacts through the same adapter
boundary as inspect and pull.

## Scope

- extend `OciArtifactAdapter` with push
- implement push through local `oras`
- require explicit push intent
- keep digest-pinned refs invalid as destinations
- report immutable pushed digest
- ensure userinfo and credentials are redacted from reports and errors
- add fake adapter tests before any live-registry proof
- wire push into `artifact capture --push`

## Non-Goals

- no credential manager
- no implicit public publish
- no overwrite flag unless required by the chosen implementation
- no production data mutation
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when `artifact capture --push` can publish a staged
artifact through the adapter boundary, returns the pushed digest, and fake
transport tests prove no credentials leak.

## Closeout

- `OciArtifactAdapter` now exposes `push`
- ORAS-backed push is wired through `artifact capture --push`
- capture still stages locally before pushing
- digest-pinned destination refs stay invalid
- capture reports `planned=false`, `pushed=true`, and the immutable pushed
  digest when push succeeds
- fake adapter tests prove command-layer push reporting without a live registry
- process errors still sanitize raw OCI refs/userinfo

## Next Task

Card
[`428-decide-container-data-dump-live-push-boundary.md`](./428-decide-container-data-dump-live-push-boundary.md).
