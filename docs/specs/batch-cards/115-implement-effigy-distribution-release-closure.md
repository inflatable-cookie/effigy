# 115 Implement Effigy Distribution Release Closure

Status: ready
Updated: 2026-04-15
Roadmap: `g02.007`
Spec: `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`

## Objective

Carry the shipped optional distribution surface through the actual Effigy
release-closure batch now that the release-prep hardening work is complete.

This card is active again now that the modularization bar for pre-`v0.3`
release work is met.

## In Scope

- prepare the Effigy release surfaces for the shipped distribution and
  container-backed release-prep boundary
- update changelog/release-note/log state as needed for the release checkpoint
- run the bounded release-readiness path short of any human-only irreversible
  release action unless explicitly requested in the batch
- leave the lane positioned for either final human-approved release execution
  or the next rollout batch

## Out Of Scope

- consumer rollout across other repos
- the post-release architecture modularization lane
- workflow edits
- publishing a release without explicit human approval

## Acceptance Criteria

- Effigy's release-closure state honestly reflects the shipped optional
  distribution boundary and the new Linux rehearsal support
- the repo is positioned for explicit human-approved release execution without
  further hardening detours
- the next move is explicit

## Validation

- bounded release-readiness validation for this batch
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Decide whether the repo is now ready for explicit human-approved release
execution or still needs one tighter release-prep batch.
