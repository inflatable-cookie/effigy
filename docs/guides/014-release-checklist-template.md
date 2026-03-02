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

- [ ] Consolidated release gate pass:
  - [ ] `cargo qa-release`
- [ ] `cargo fmt` clean.
- [ ] `cargo test` passes.
- [ ] Local quality gates pass:
  - [ ] `cargo qa`
- [ ] Docs link integrity check passes:
  - [ ] `cargo qa-docs`
- [ ] CI gate is green before merge/tag:
  - [ ] `json-contracts / Validate docs links`
  - [ ] `json-contracts / Validate JSON contracts`
- [ ] Focused smoke checks pass in active workspace(s):
  - [ ] `effigy help`
  - [ ] `effigy tasks`
  - [ ] `effigy catalog-a/tasks`
  - [ ] `effigy test --plan`
  - [ ] `effigy catalog-a/test --plan`

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
- [ ] Distribution metadata validation passes:
  - [ ] `./scripts/check-distribution-metadata.sh --tag v0.__.__`
- [ ] Tag points to intended commit.
- [ ] Publish command executed/queued.
- [ ] Install validated from git tag:
  - [ ] `./scripts/check-release-install-from-tag.sh --tag v0.__.__`
- [ ] Install validated from published crate.
- [ ] First-publish artifact bundle captured:
  - [ ] `./scripts/check-distribution-first-publish.sh --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__`
  - [ ] `./scripts/validate-distribution-artifacts.sh --artifacts-dir ./artifacts/distribution-v0.__.__`

### Homebrew
- [ ] Formula updated to new version.
- [ ] Checksums updated.
- [ ] Tap commit merged/pushed.
- [ ] Fresh install + upgrade path validated.
- [ ] Tap automation workflow ran for release tag and attached evidence.

## 5) Rollback Preparedness

- [ ] Previous known-good version documented.
- [ ] Rollback command/instructions prepared.
- [ ] Communication template prepared for incident/hotfix.

## 6) Post-Release Validation

- [ ] Validate install on at least one clean machine/session.
- [ ] Validate prefixed built-ins still route correctly.
- [ ] Validate `test` summary output in compact mode.
- [ ] Open dated checkpoint report in `docs/reports/`.
  - [ ] `./scripts/generate-distribution-closeout-report.sh --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__ [--expect-homebrew]`

## 7) Sign-off

- [ ] Release approved by owner.
- [ ] Release announcement sent.
- [ ] Backlog/roadmap status updated.

---

## Related Guides

- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`../roadmap/backlog/release-contract-v0.md`](../roadmap/backlog/release-contract-v0.md)
- [`../roadmap/backlog/distribution-channels.md`](../roadmap/backlog/distribution-channels.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)

## Next Step

After running this checklist for a release, publish a dated report under `docs/reports/` and link it from your release PR.
