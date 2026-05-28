# 042 - Artifact Substrate For Seed Apply And Capture Workflows Strict Lane

Roadmap: [`g03.036`](../roadmaps/g03/036-artifact-substrate-for-seed-apply-and-capture-workflows.md)

Status: Complete
Owner: Platform
Created: 2026-05-06

## Purpose

Create a standalone Effigy artifact substrate for local and OCI data payloads
used by bootstrap, container data seed/dump, and Example App UAT apply/capture
workflows.

This lane keeps the artifact surface separate from config bundles and separate
from the container built-in. Artifact handling owns transport, staging,
metadata, digest reporting, and operation records. App code owns migration
semantics.

## Hard Boundaries

- do not edit `.github/workflows/`
- do not initiate release commands
- do not move Example App migration/coercion logic into Effigy
- do not make OCI refs implicit; first-round OCI refs use `oci://`
- do not log registry credentials or tokens
- preserve local SQL seed behavior while widening it through artifact staging
- keep the first crate dependency-light

## Current Ready Card

None.

## Execution Chain

- `415` complete: artifact contract and Example App boundary
- `416` complete: scaffold `effigy-artifacts`
- `417` complete: local artifact staging for seed inputs
- `418` complete: OCI pull/inspect/stage core
- `419` complete: seed/dump apply/capture integration
- `420` complete: Example App proof and closeout
- `421` complete: public artifact inspect/stage and Farmyard handoff output
- `422` complete: live OCI transport and private-registry proof
- `423` complete: wire OCI artifact refs into seed surfaces and park dump push
- `424` complete: plan OCI capture/push for UAT snapshots
- `425` complete: implement local artifact capture with planned OCI push
- `426` complete: wire planned artifact capture into container data dump
- `427` complete: implement live OCI push through artifact adapter
- `428` complete: initial conservative boundary kept container data dump
  planned-only
- `429` complete: close artifact substrate lane
- `430` complete: implement explicit container data dump live OCI push after
  user opted into the one-command workflow

## Exit Condition

This lane closes when local SQL and OCI artifact sources resolve through one
metadata/staging model, seed/dump surfaces can consume that model, UAT
apply/capture reports are defined, and the Example App proof keeps app-owned
migration logic outside Effigy.

## Next Task

Stop in planning and choose the next roadmap deliberately.
