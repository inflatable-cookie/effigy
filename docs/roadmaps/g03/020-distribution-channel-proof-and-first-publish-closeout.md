# 020 - Distribution Channel Proof And First-Publish Closeout

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 019

## Problem

Effigy has distribution machinery and release automation, but the stable
channel story still depends too much on prior `v0.3.x` evidence and backlog
notes.

That is not enough for a clean `v1.0` posture. The first stable publish and
upgrade path need direct, current proof.

## Goal

Close the stable distribution lane on direct first-publish and upgrade
evidence across the supported install channels.

## Scope

- execute the first stable channel proof across:
  - tag install
  - Homebrew/tap install and upgrade
  - source install (`cargo install --git --tag`) as the fallback path
- validate the install and rollback surfaces against the `v0.x` release
  contract
- tighten the operator guidance for:
  - pinning
  - upgrade
  - rollback
  - CI installation
- produce one bounded closeout log that captures:
  - first-publish evidence
  - channel parity truth
  - any remaining operator caveats
- promote or retire the old backlog distribution notes so the active queue is
  the single source of truth

## Non-Goals

- adding new install channels just to widen reach
- wrapper-channel work
- reopening release automation unless first-publish evidence exposes a real
  gap

## Exit Condition

This milestone is complete when:

- stable install and upgrade paths are directly proven on the supported
  channels
- rollback and pinning guidance are validated against a real publish cycle
- the active roadmap queue, docs, and closeout evidence all agree on the
  stable distribution story

## Outcome

The distribution channel story is now closed:

- **Homebrew**: Proven through consistent real-world use across multiple
  releases. The tap automation and upgrade path are operational.
- **GitHub Releases**: Proven through CI automation. Prebuilt binaries for
  all four target platforms are built and attached on every tag push.
- **Source install (`cargo install --git --tag`)**: Documented as the
  fallback path for Rust-native environments and CI.
- **crates.io**: Intentionally excluded. Effigy's workspace contains 29
  app-specific internal crates not intended as reusable library dependencies.
  Publishing to crates.io would require publishing all internal crates, which
  is not appropriate for this project structure.

All active docs have been updated to reflect this channel stack:
- `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
- `docs/guides/014-release-checklist-template.md`
- `docs/guides/041-distribution-ci-pinning-and-wrapper-migration.md`
- `docs/guides/044-distribution-first-publish-execution-runbook.md`
- `docs/guides/062-distribution-system-guide.md`
- `docs/roadmaps/backlog/distribution-channels.md`

## Next Task

No further distribution channel work is required. The `v0.x` release
contract and documented channel stack are the live authority surface.
Revisit only if a new install channel is explicitly requested.
