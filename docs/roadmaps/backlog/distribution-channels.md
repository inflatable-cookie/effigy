# Distribution Channels (Backlog)

Status: Promoted
Owner: Platform
Created: 2026-02-27
Depends on: initial feature freeze + release contract

Superseded by:
- `g03.020-distribution-channel-proof-and-first-publish-closeout`

## 1) Context

Effigy is still evolving, so distribution planning should be staged and reversible. This backlog item defines release channels and operational requirements, without locking release dates yet.

The active execution target for this topic now lives in `g03`, so this file
should be treated as historical backlog evidence rather than the live queue.

## 2) Goals

- [ ] Define a stable channel strategy that supports rapid iteration now and smooth upgrades later.
- [ ] Keep one canonical binary source of truth (no parallel runtime implementations).
- [ ] Support easy install/upgrade for macOS-first teams.
- [ ] Support deterministic installs for CI and headless environments.
- [ ] Document rollout, rollback, and version pinning policy.

## 3) Non-Goals

- [ ] No feature freeze decision in this backlog item.
- [ ] No release-date commitment.
- [ ] No long-term support matrix yet.

## 4) Proposed Channel Stack

1. Dev channel (current):
- `cargo run --manifest-path ... --bin effigy -- ...`
- Purpose: immediate local iteration while behavior is still moving.

2. Source install channel:
- `cargo install --git https://github.com/inflatable-cookie/effigy.git --tag vX.Y.Z`
- Purpose: reproducible source builds in Rust-native environments and CI.
- Not crates.io: Effigy's workspace contains app-specific internal crates not
  intended as reusable library dependencies.

3. Homebrew channel:
- custom tap/formula for `brew install`/`brew upgrade`.
- Purpose: best default DX for macOS users.

4. Optional wrapper channel (later):
- npm/other thin wrappers only if needed for JS-first workflows.
- Purpose: convenience only; still delegates to canonical binary.

## 5) Execution Plan

### Phase A - Release Contract and Artifact Policy
- [x] Define minimum release contract for first public version (`v0.1.x` target scope).
- [x] Define semver policy and compatibility expectations.
- [x] Define rollback and hotfix process.
- [x] Define changelog format and release notes template.

### Phase B - Source Install Path
- [x] Tag-driven source install (`cargo install --git --tag`) documented and validated.
- [x] Release checklist includes source-install verification.
- [ ] Not applicable: Effigy does not publish to crates.io because its workspace
  contains app-specific internal crates not intended as reusable library dependencies.

### Phase C - Homebrew Path
- [x] Create and validate tap repository/formula workflow.
- [x] Automate formula bump on release tags.
- [x] Define bottle/checksum/update strategy.
  - implementation hooks:
    - `.github/workflows/release-binaries.yml` (includes homebrew metadata generation and tap PR automation)

### Phase D - CI + Team Adoption
- [x] Add pinned-version install snippets for CI.
- [x] Add bootstrap docs for local dev + fallback channels.
- [x] Add upgrade guide for existing projects using `bun effigy` wrappers.

### Phase E - Optional Wrapper Evaluation
- [x] Reassess need for npm wrapper after crates + brew are stable.
- [x] If needed, implement thin wrapper policy with strict binary delegation.

## 6) Acceptance Criteria

- [x] One-command install exists for macOS-default users (Homebrew).
- [x] Source-install path documented for Rust-native environments (`cargo install --git --tag`).
- [ ] Version pinning and rollback are documented and tested.
- [ ] Release and upgrade flow is repeatable from CI.
- [x] Channel docs clearly distinguish dev channel vs stable channels.

### Current Closeout Status (2026-03-02)

- Completed now:
  - channel documentation coverage for dev vs stable paths
  - CI pinning guidance and wrapper migration policy
  - Homebrew workflow and release automation policy
- Still blocked until publish-cycle execution:
  - release tag install validation on an actual published release tag
  - full end-to-end channel matrix execution evidence from one release cycle
  - source-install path validated against a real tag

## 7) Risks and Mitigations

- [ ] Risk: releasing too early causes breaking upgrade churn.
  - Mitigation: gate channel rollout on explicit release contract.
- [ ] Risk: multiple channels drift in behavior.
  - Mitigation: single binary artifact source and automated channel updates.
- [ ] Risk: adoption friction from mixed legacy invocations.
  - Mitigation: migration guide and phased fallback retention.

## 8) Deliverables

- [x] Release contract doc + checklist.
- [x] Source install path documented (`cargo install --git --tag`).
- [x] Homebrew tap + automation workflow.
- [x] CI install recipes and migration guidance.

## 9) First-Publish Execution Gate

Before marking this backlog item complete, execute:
- [`../../guides/044-distribution-first-publish-execution-runbook.md`](../../guides/044-distribution-first-publish-execution-runbook.md)
- `./scripts/check-distribution-first-publish.sh --tag <tag> --artifacts-dir <dir>`
- `./scripts/generate-distribution-closeout-log.sh --tag <tag> --artifacts-dir <dir> [--expect-homebrew]`
- one dated acceptance-closeout log with channel matrix evidence

## 10) Vision Target Movement Criteria

Primary tags:
- `RELEASE`
- `MAINT`

Target envelope:
- Effigy distribution is repeatable across channels with one canonical binary source and reversible rollout controls.

Promotion signals:
- one completed publish cycle includes validated install evidence for tag install, source install, and Homebrew flow
- rollback path is executed or dry-run validated and documented in a dated log
- release contract checklist is fully linked to channel artifacts with no open blockers
