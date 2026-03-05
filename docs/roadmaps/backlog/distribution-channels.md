# Distribution Channels (Backlog)

Status: Backlog
Owner: Platform
Created: 2026-02-27
Depends on: initial feature freeze + release contract

## 1) Context

Effigy is still evolving, so distribution planning should be staged and reversible. This backlog item defines release channels and operational requirements, without locking release dates yet.

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

2. Rust install channel:
- `cargo install` (first via git/tag, then crates.io).
- Purpose: reproducible installs in Rust-native environments and CI.

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

### Phase B - Crates Path
- [x] Finalize crate metadata and publish readiness checks.
- [x] Add tag-driven release checklist for crates publication.
- [ ] Validate `cargo install` flows from tag and from crates.io.
  - [x] tag-based install validation automated in release gates
  - [ ] crates.io install validation (pending first publish cycle)

### Phase C - Homebrew Path
- [x] Create and validate tap repository/formula workflow.
- [x] Automate formula bump on release tags.
- [x] Define bottle/checksum/update strategy.
  - implementation hooks:
    - `.github-bak/workflows/homebrew-tap-metadata.yml` (metadata artifact generation)
    - `.github-bak/workflows/homebrew-tap-formula-pr.yml` (tap PR automation from artifact)

### Phase D - CI + Team Adoption
- [x] Add pinned-version install snippets for CI.
- [x] Add bootstrap docs for local dev + fallback channels.
- [x] Add upgrade guide for existing projects using `bun effigy` wrappers.

### Phase E - Optional Wrapper Evaluation
- [x] Reassess need for npm wrapper after crates + brew are stable.
- [x] If needed, implement thin wrapper policy with strict binary delegation.

## 6) Acceptance Criteria

- [ ] One-command install exists for both Rust-native and macOS-default users.
- [ ] Version pinning and rollback are documented and tested.
- [ ] Release and upgrade flow is repeatable from CI.
- [x] Channel docs clearly distinguish dev channel vs stable channels.

### Current Closeout Status (2026-03-02)

- Completed now:
  - channel documentation coverage for dev vs stable paths
  - CI pinning guidance and wrapper migration policy
  - Homebrew workflow and release automation policy
- Still blocked until publish-cycle execution:
  - crates.io install validation
  - release tag install validation on an actual published release tag
  - full end-to-end channel matrix execution evidence from one release cycle

## 7) Risks and Mitigations

- [ ] Risk: releasing too early causes breaking upgrade churn.
  - Mitigation: gate channel rollout on explicit release contract.
- [ ] Risk: multiple channels drift in behavior.
  - Mitigation: single binary artifact source and automated channel updates.
- [ ] Risk: adoption friction from mixed legacy invocations.
  - Mitigation: migration guide and phased fallback retention.

## 8) Deliverables

- [x] Release contract doc + checklist.
- [x] Crates publication workflow.
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
- one completed publish cycle includes validated install evidence for tag install, crates install, and Homebrew flow
- rollback path is executed or dry-run validated and documented in a dated log
- release contract checklist is fully linked to channel artifacts with no open blockers
