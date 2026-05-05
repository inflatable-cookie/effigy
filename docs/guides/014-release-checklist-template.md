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

- [ ] Distribution preflight passes:
  - [ ] `effigy distribution preflight --tag v0.__.__`
- [ ] Safe release simulation passes:
  - [ ] `effigy release simulate`
- [ ] Release readiness check passes:
  - [ ] `effigy release status --check-gates`
- [ ] Consolidated release gate pass:
  - [ ] `effigy release gates`
- [ ] `cargo fmt` clean.
- [ ] `cargo test` passes.
- [ ] Local quality gates pass:
  - [ ] `effigy qa`
- [ ] Docs link integrity check passes:
  - [ ] `effigy qa:docs`
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
- [ ] Planned version reviewed in built-in prepare preview:
  - [ ] `effigy release prepare --plan`
- [ ] Changelog entry drafted.
- [ ] Root `README.md` and other front-door install examples updated when they
  pin a specific release tag/version.
- [ ] Release notes drafted:
  - [ ] highlights
  - [ ] vision target delta (tags, movement, remaining gap)
  - [ ] breaking changes (if any)
  - [ ] migration steps

## 4) Built-In Release Flow

- [ ] Operator path chosen explicitly:
  - [ ] built-in commands are the primary path for this release
- [ ] Prepared-state apply succeeds:
  - [ ] `effigy release prepare --yes --check-gates`
- [ ] Execute preflight succeeds:
  - [ ] `effigy release execute --plan`
- [ ] Human approval recorded before irreversible step.
- [ ] Final execute succeeds:
  - [ ] `effigy release execute --yes`

## 5) Channel Artifacts

### GitHub Releases and Source Install
- [ ] Package metadata verified.
- [ ] Distribution metadata validation passes:
  - [ ] `effigy distribution validate-metadata --tag v0.__.__`
- [ ] Tag points to intended commit.
- [ ] Install validated from git tag:
  - [ ] `effigy release verify-install --tag v0.__.__`
- [ ] Source-install path validated:
  - [ ] `cargo install --locked --git https://github.com/inflatable-cookie/effigy.git --tag v0.__.__ effigy --force`
- [ ] First-publish artifact bundle captured:
  - [ ] `effigy distribution first-publish --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__`
  - [ ] `effigy distribution validate-artifacts --artifacts-dir ./artifacts/distribution-v0.__.__`

### Homebrew
- [ ] Formula updated to new version.
- [ ] Checksums updated.
- [ ] Tap commit merged/pushed.
- [ ] Fresh install + upgrade path validated.
- [ ] Tap automation workflow ran for release tag and attached evidence.

## 6) Rollback Preparedness

- [ ] Previous known-good version documented.
- [ ] Rollback command/instructions prepared.
- [ ] Communication template prepared for incident/hotfix.

## 7) Post-Release Validation

- [ ] Validate install on at least one clean machine/session.
- [ ] Validate prefixed built-ins still route correctly.
- [ ] Validate `test` summary output in compact mode.
- [ ] Open dated checkpoint log in `docs/logs/YYYY-MM/`.
  - [ ] `effigy distribution generate-closeout --tag v0.__.__ --artifacts-dir ./artifacts/distribution-v0.__.__ [--expect-homebrew]`

## 8) Sign-off

- [ ] Release approved by owner.
- [ ] Release announcement sent.
- [ ] Backlog/roadmap status updated.

---

## Related Guides

- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`054-release-checkpoint-log-template.md`](./054-release-checkpoint-log-template.md)
- [`../roadmaps/backlog/release-contract-v0.md`](../roadmaps/backlog/release-contract-v0.md)
- [`../roadmaps/backlog/distribution-channels.md`](../roadmaps/backlog/distribution-channels.md)
- [`042-homebrew-tap-and-release-automation.md`](./042-homebrew-tap-and-release-automation.md)

## Next Step

After running this checklist for a release, publish a dated log under `docs/logs/YYYY-MM/` and link it from your release PR.
