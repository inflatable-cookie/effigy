# 418 - OCI Pull Inspect And Stage

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06
Completed: 2026-05-06

## Goal

Choose and implement the first OCI inspect/pull/stage path for artifact refs.

## Scope

- decide implementation path:
  - Rust crate
  - existing OCI tool
  - narrow adapter over the existing Underlay devtools shape
- keep `oci://` refs explicit
- avoid logging credentials or tokens
- capture immutable digest when available
- stage pulled payloads into the same `effigy.artifact.v1` model used by local
  artifacts
- add tests for request/report shaping and token redaction

## Non-Goals

- no public CLI promotion unless needed as a private test harness
- no bootstrap/container data seed wiring yet
- no Acowtancy file edits
- no dynamic OCI plugin loading

## Decision Pressure

The implementation should support private registries for UAT, but this card
does not need to prove a live private registry. Prefer a small adapter boundary
that can be tested without network and widened later with live proof fixtures.

## Exit Condition

This card is complete when Effigy has a chosen OCI adapter path, request/report
types for inspect/pull/stage, tests for safe report rendering, and a staged OCI
fixture can flow into `StagedArtifactReport`.

## Closeout

Chose an adapter boundary first. Live registry transport stays behind
`OciArtifactAdapter`; seed/dump code should depend on artifact request/report
types and staged reports, not on a specific registry client.

Added:

- OCI inspect and pull request models
- OCI descriptor and pull report models
- `OciArtifactAdapter`
- safe reportable ref redaction
- OCI fixture staging into `effigy.artifact.v1`

Validation passed:

```sh
cargo test -p effigy-artifacts
```

## Next Task

Card [`419-seed-dump-apply-capture-integration.md`](./419-seed-dump-apply-capture-integration.md).
