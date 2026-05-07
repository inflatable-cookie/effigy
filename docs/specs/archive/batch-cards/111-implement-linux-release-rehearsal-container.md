# 111 Implement Linux Release Rehearsal Container

Status: complete
Updated: 2026-04-15
Roadmap: `g02.007`
Spec: `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`

## Objective

Give Effigy one honest local Linux release-rehearsal path so pre-release prep
can exercise the Linux build directly instead of depending only on CI.

## In Scope

- define one Effigy-owned Linux build test container in the repo manifest
- make the container usable through the shipped `effigy container` surface
- provide one repo-owned task or release-prep entrypoint that uses that
  container for Linux build rehearsal
- exercise the Linux build and GLIBC floor path locally through that container
- document the supported operator path clearly enough for release prep

## Out Of Scope

- changing GitHub workflows
- replacing the existing release CI matrix
- broad multi-container release orchestration
- consumer rollout beyond Effigy itself

## Acceptance Criteria

- Effigy has one named local Linux rehearsal container that can be started on a
  machine with the new container surface
- pre-release prep can use that container to build the Linux binary and run the
  GLIBC floor check locally
- the local proof reduces blind trust in CI without over-claiming full release
  parity
- docs and lane state reflect the new release-prep boundary honestly

## Validation

- targeted validation for the new container/rehearsal path
- one real local proof run of the Linux build container path
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `112-decide-post-linux-rehearsal-release-boundary.md` to decide
whether the Linux rehearsal proof is strong enough to move directly into
Effigy release closure or still needs one more bounded release-hardening batch.
