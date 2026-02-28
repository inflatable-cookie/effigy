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
- [ ] Define minimum release contract for first public version (`v0.1.x` target scope).
- [ ] Define semver policy and compatibility expectations.
- [ ] Define rollback and hotfix process.
- [ ] Define changelog format and release notes template.

### Phase B - Crates Path
- [ ] Finalize crate metadata and publish readiness checks.
- [ ] Add tag-driven release checklist for crates publication.
- [ ] Validate `cargo install` flows from tag and from crates.io.

### Phase C - Homebrew Path
- [ ] Create and validate tap repository/formula workflow.
- [ ] Automate formula bump on release tags.
- [ ] Define bottle/checksum/update strategy.

### Phase D - CI + Team Adoption
- [ ] Add pinned-version install snippets for CI.
- [ ] Add bootstrap docs for local dev + fallback channels.
- [ ] Add upgrade guide for existing projects using `bun effigy` wrappers.

### Phase E - Optional Wrapper Evaluation
- [ ] Reassess need for npm wrapper after crates + brew are stable.
- [ ] If needed, implement thin wrapper policy with strict binary delegation.

## 6) Acceptance Criteria

- [ ] One-command install exists for both Rust-native and macOS-default users.
- [ ] Version pinning and rollback are documented and tested.
- [ ] Release and upgrade flow is repeatable from CI.
- [ ] Channel docs clearly distinguish dev channel vs stable channels.

## 7) Risks and Mitigations

- [ ] Risk: releasing too early causes breaking upgrade churn.
  - Mitigation: gate channel rollout on explicit release contract.
- [ ] Risk: multiple channels drift in behavior.
  - Mitigation: single binary artifact source and automated channel updates.
- [ ] Risk: adoption friction from mixed legacy invocations.
  - Mitigation: migration guide and phased fallback retention.

## 8) Deliverables

- [ ] Release contract doc + checklist.
- [ ] Crates publication workflow.
- [ ] Homebrew tap + automation workflow.
- [ ] CI install recipes and migration guidance.
