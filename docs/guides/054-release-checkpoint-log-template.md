# 054 - Release Checkpoint Log Template

Use this guide to create the one dated maintainer checkpoint log for a real
Effigy release. The goal is to keep release evidence, distribution evidence,
and wrapper-retirement decisions in a single artifact.

## 1) When To Use It

Create a checkpoint log when:

- a real Effigy release is being cut
- the release has distribution/channel evidence to capture
- maintainers may evaluate wrapper retirement for that release window

Write the log under:

- `docs/logs/YYYY-MM/DD-HHMMSS-release-checkpoint-vX.Y.Z.md`

## 2) Template

```md
# Release Checkpoint - vX.Y.Z

Date: YYYY-MM-DD
Owner: <team/person>
Related roadmap: g01.027 release orchestration system
Release: vX.Y.Z

## Summary
- Short release summary.
- Why this release matters.

## Vision Target Delta
- Primary tags: `OPERATE`, `RELEASE`, `MAINT`
- Movement: `...` -> `...`
- Remaining gap: `...` or `None`

## Built-In Release Path
- `effigy release simulate`
  - result: ...
- `effigy release status --check-gates`
  - result: ...
- `effigy release prepare --yes --check-gates`
  - result: ...
- `effigy release execute --yes`
  - result: ...

## Hosted Workflow Results
- `CI`
  - run/link: ...
  - result: ...
- `JSON Contracts`
  - run/link: ...
  - result: ...
- `Release Binaries`
  - run/link: ...
  - result: ...

## Distribution Evidence
- artifacts directory: `./artifacts/distribution-vX.Y.Z`
- `effigy release verify-install --tag vX.Y.Z`
  - result: ...
- `./scripts/check-distribution-first-publish.sh --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`
  - result: ...
- `effigy distribution validate-artifacts --artifacts-dir ./artifacts/distribution-vX.Y.Z`
  - result: ...
- `effigy distribution generate-closeout --tag vX.Y.Z --artifacts-dir ./artifacts/distribution-vX.Y.Z`
  - result: ...

## Release Notes
- changelog extraction baseline:
  - `effigy changelog extract CHANGELOG.md --version X.Y.Z`
- published release notes link: ...
- migration notes summary: ...

## Wrapper Retirement Record
- Release evaluated: `vX.Y.Z`
- Prior built-in release in comparison window: `v0.__.__`
- Built-in release path used for both releases:
  - [ ] `effigy release prepare`
  - [ ] `effigy release execute`
- Wrapper fallback used:
  - [ ] no
  - [ ] yes, with explanation recorded
- Hosted release workflows green on both releases:
  - [ ] `CI`
  - [ ] `JSON Contracts`
  - [ ] `Release Binaries`
- Tag install validation green on both releases:
  - [ ] `effigy release verify-install`
- Any active CI/docs/downstream contract still points to wrapper scripts:
  - [ ] no
  - [ ] yes, list them explicitly
- Maintainer decision:
  - [ ] keep wrappers for another release cycle
  - [ ] retire `scripts/prepare-release.sh`
  - [ ] retire `scripts/check-release-gates.sh`
  - [ ] retire `scripts/check-release-install-from-tag.sh`
- Decision owner: `name/team`
- Notes:

## Risks / Follow-ups
- Remaining release risks:
  - ...
- Follow-up tasks:
  - ...

## Next
- State the next release-ops or cleanup action.
```

## 3) Authoring Rules

- Keep exact workflow names and links.
- Keep exact commands for local validation.
- Link to the generated distribution closeout log when one exists.
- If wrapper fallback was used, say exactly why and where.
- If wrappers are kept, record why the evidence was insufficient to retire them.

## 4) Evidence Sources

Pull evidence from:

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)
- [`053-release-wrapper-retirement-record-template.md`](./053-release-wrapper-retirement-record-template.md)
- workflow runs and published release artifacts

## Related Guides

- [`014-release-checklist-template.md`](./014-release-checklist-template.md)
- [`036-release-notes-authoring-template-and-examples.md`](./036-release-notes-authoring-template-and-examples.md)
- [`044-distribution-first-publish-execution-runbook.md`](./044-distribution-first-publish-execution-runbook.md)
- [`049-ci-binary-distribution-and-release-protocol.md`](./049-ci-binary-distribution-and-release-protocol.md)
- [`053-release-wrapper-retirement-record-template.md`](./053-release-wrapper-retirement-record-template.md)

## Next Step

When the next real Effigy release closes, use this template for the dated
checkpoint log and link that log from `docs/logs/README.md`.
