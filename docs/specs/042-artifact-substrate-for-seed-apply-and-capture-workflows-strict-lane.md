# 042 - Artifact Substrate For Seed Apply And Capture Workflows Strict Lane

Roadmap: [`g03.036`](../roadmaps/g03/036-artifact-substrate-for-seed-apply-and-capture-workflows.md)

Status: Active
Owner: Platform
Created: 2026-05-06

## Purpose

Create a standalone Effigy artifact substrate for local and OCI data payloads
used by bootstrap, container data seed/dump, and Acowtancy UAT apply/capture
workflows.

This lane keeps the artifact surface separate from config bundles and separate
from the container built-in. Artifact handling owns transport, staging,
metadata, digest reporting, and operation records. App code owns migration
semantics.

## Hard Boundaries

- do not edit `.github/workflows/`
- do not initiate release commands
- do not move Acowtancy migration/coercion logic into Effigy
- do not make OCI refs implicit; first-round OCI refs use `oci://`
- do not log registry credentials or tokens
- preserve local SQL seed behavior while widening it through artifact staging
- keep the first crate dependency-light

## Current Ready Card

None. Card
[`419-seed-dump-apply-capture-integration.md`](./batch-cards/419-seed-dump-apply-capture-integration.md)
is in progress.

## Execution Chain

- `415` complete: artifact contract and Acowtancy boundary
- `416` complete: scaffold `effigy-artifacts`
- `417` complete: local artifact staging for seed inputs
- `418` complete: OCI pull/inspect/stage core
- `419` in progress: seed/dump apply/capture integration
- `420` blocked by `419`: Acowtancy proof and closeout

## Exit Condition

This lane closes when local SQL and OCI artifact sources resolve through one
metadata/staging model, seed/dump surfaces can consume that model, UAT
apply/capture reports are defined, and the Acowtancy proof keeps app-owned
migration logic outside Effigy.

## Next Task

Continue card
[`419-seed-dump-apply-capture-integration.md`](./batch-cards/419-seed-dump-apply-capture-integration.md)
after the unrelated container/cache compile blockers are resolved or isolated.
