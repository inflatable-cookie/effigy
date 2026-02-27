# 014 - Release Checklist Template

Use this template for every release tag while Effigy is in `v0.x`.

Copy this into a dated release note/checklist and check items as you execute.

---

# Effigy Release Checklist

Release version: `v0.__.__`  
Release date: `YYYY-MM-DD`  
Owner: `name/team`

## 1) Scope and Risk

- [ ] Confirm release scope summary is written (what changed, why it matters).
- [ ] Confirm migration notes are written for any behavior/config changes.
- [ ] Confirm known risks are listed with mitigation/rollback notes.

## 2) Quality Gates

- [ ] `cargo fmt` clean.
- [ ] `cargo test` passes.
- [ ] Focused smoke checks pass in active workspace(s):
  - [ ] `effigy help`
  - [ ] `effigy tasks`
  - [ ] `effigy farmyard/tasks`
  - [ ] `effigy test --plan`
  - [ ] `effigy farmyard/test --plan`

## 3) Versioning and Notes

- [ ] Version bump matches policy (`PATCH` vs `MINOR`) from release contract.
- [ ] Changelog entry drafted.
- [ ] Release notes drafted:
  - [ ] highlights
  - [ ] breaking changes (if any)
  - [ ] migration steps

## 4) Channel Artifacts

### Crates
- [ ] `Cargo.toml` metadata verified.
- [ ] Tag points to intended commit.
- [ ] Publish command executed/queued.
- [ ] Install validated from published crate.

### Homebrew
- [ ] Formula updated to new version.
- [ ] Checksums updated.
- [ ] Tap commit merged/pushed.
- [ ] Fresh install + upgrade path validated.

## 5) Rollback Preparedness

- [ ] Previous known-good version documented.
- [ ] Rollback command/instructions prepared.
- [ ] Communication template prepared for incident/hotfix.

## 6) Post-Release Validation

- [ ] Validate install on at least one clean machine/session.
- [ ] Validate prefixed built-ins still route correctly.
- [ ] Validate `test` summary output in compact mode.
- [ ] Open dated checkpoint report in `docs/reports/`.

## 7) Sign-off

- [ ] Release approved by owner.
- [ ] Release announcement sent.
- [ ] Backlog/roadmap status updated.

---

Related docs:
- [Release Contract (v0.x)](../roadmap/backlog/release-contract-v0.md)
- [Distribution Channels Backlog](../roadmap/backlog/distribution-channels.md)
