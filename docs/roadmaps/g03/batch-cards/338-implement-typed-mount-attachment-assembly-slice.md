# 338 Implement Typed Mount Attachment Assembly Slice

Status: archived
Updated: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`

## Objective

Move generated media and host mount attachment onto the typed generated-compose
assembly model.

## In Scope

- extend the typed generated-compose model in `effigy-containers`
- migrate these remaining generated-compose seams off YAML reparsing:
  - generated media mount attachment
  - generated host mount attachment
- give repo-root attachment one typed owner instead of rediscovering it from
  raw YAML in each policy helper
- preserve current generated-compose behavior for:
  - duplicate mount suppression
  - repo-root-only attachment rules
  - current error surfaces where no eligible service exists

## Out Of Scope

- workspace-specific compose rewrite helpers
- catalog format redesign
- new container features

## Acceptance Criteria

- generated media and host mount policy no longer reparse compose YAML as
  their working data model
- repo-root-attached service detection has one typed owner for this path
- generated-compose policy coverage still passes for:
  - media mounts
  - host mounts
  - composer home / SSH / other downstream volume consumers

## Validation

- targeted `effigy-containers` tests for the typed mount-attachment path
- targeted generated-compose integration tests for mount-bearing services
- `./target/debug/effigy docs check-paths docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md docs/roadmaps/g03/batch-cards/338-implement-typed-mount-attachment-assembly-slice.md docs/roadmaps/g03/batch-cards/339-decide-post-typed-mount-attachment-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/014-container-assembly-model-and-single-pass-compose-emission.md`

## Next Task

Closed. Execute `339`.
