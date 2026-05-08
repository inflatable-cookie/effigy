# 336 Implement Typed Container Assembly Foundation

Status: archived
Updated: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`

## Objective

Introduce the first real typed container/runtime assembly model and move one
 high-value compose policy seam onto it.

## In Scope

- define shared internal assembly types for:
  - service declarations
  - ports and published ports
  - mounts
  - DNS/alias metadata
  - shared-service metadata
  - assembly-owned policy flags needed by generated compose
- build one typed assembly construction path inside `effigy-containers`
- migrate the first high-value policy seam fully off YAML rewrite passes:
  - generated port publication policy
  - or shared-service env/binding policy
  - whichever is the cleaner first full conversion once the assembly type is
    in place
- emit compose YAML once from the typed model for that migrated path
- add targeted tests that assert typed assembly truth and final emitted shape

## Out Of Scope

- migrating every compose rewrite path in one batch
- workspace/runtime orchestrator splitting
- catalog format redesign
- new container features

## Acceptance Criteria

- a typed container assembly model exists and is the real owner of at least
  one meaningful generated-compose policy seam
- the migrated seam no longer depends on reparsing YAML strings as its working
  data model
- emitted compose output still preserves current shipped behavior for that
  seam
- targeted tests prove both typed assembly truth and emitted compose shape

## Validation

- targeted `effigy-containers` assembly/policy tests
- targeted generated-compose integration tests for the migrated seam
- `./target/debug/effigy docs check-paths docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md docs/roadmaps/g03/batch-cards/336-implement-typed-container-assembly-foundation.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/014-container-assembly-model-and-single-pass-compose-emission.md`

## Next Task

Closed. Execute `337`.
