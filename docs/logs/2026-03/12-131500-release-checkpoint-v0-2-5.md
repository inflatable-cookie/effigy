# Release Checkpoint - v0.2.5

Date: 2026-03-12
Owner: effigy maintainers
Related roadmap: g01.027 release orchestration system
Release: v0.2.5

## Summary

- Effigy `v0.2.5` was the first real production release executed through the
  built-in `effigy release ...` path on the main repo.
- The release proved the built-in prepare/execute flow on the real repository,
  real tag, real hosted workflows, and real GitHub Release publication path.
- The release does not yet justify wrapper retirement because it is only the
  first production built-in release in the evaluation window.

## Vision Target Delta

- Primary tags: `OPERATE`, `RELEASE`, `MAINT`
- Movement: built-in release orchestration moved from rehearsal-proven to
  first-production-release-proven on the real Effigy repo
- Remaining gap:
  - second consecutive production release through the built-in flow
  - standardized first-publish artifact bundle evidence in the same checkpoint
  - explicit wrapper-retirement decision after the comparison window is complete

## Built-In Release Path

- `effigy release prepare --repo . --yes --check-gates`
  - result: succeeded and wrote a valid `.release-prepared.json`
- `effigy release execute --repo . --yes`
  - result: succeeded; created release commit
    `2f5592ba329fc101bc87f7fa38be0960da53ee5d` with message
    `release: v0.2.5`, pushed `main`, pushed tag `v0.2.5`, and removed
    `.release-prepared.json`
- pre-release feature batch commit:
  - `0284934` `Add built-in release orchestration workflow`

## Hosted Workflow Results

- `CI`
  - run/link:
    `https://github.com/inflatable-cookie/effigy/actions/runs/22969283606`
  - result: success
- `JSON Contracts`
  - run/link:
    `https://github.com/inflatable-cookie/effigy/actions/runs/22969283586`
  - result: success
- `Release Binaries`
  - run/link:
    `https://github.com/inflatable-cookie/effigy/actions/runs/22969284279`
  - result: success
- Published release:
  - `https://github.com/inflatable-cookie/effigy/releases/tag/v0.2.5`

## Distribution Evidence

- `effigy release verify-install --repo . --tag v0.2.5`
  - result: built-in tag-install path was proven as part of the shipped release
    surface and wrapper parity work, but a dated first-publish artifact bundle
    was not attached to this release checkpoint
- `./scripts/check-distribution-first-publish.sh --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5`
  - result: not recorded in this production checkpoint
- `effigy distribution validate-artifacts --repo . --artifacts-dir ./artifacts/distribution-v0.2.5`
  - result: not recorded in this production checkpoint
- `effigy distribution generate-closeout --repo . --tag v0.2.5 --artifacts-dir ./artifacts/distribution-v0.2.5`
  - result: not recorded in this production checkpoint

## Release Notes

- changelog extraction baseline:
  - `effigy changelog extract CHANGELOG.md --version 0.2.5`
- published release notes link:
  - `https://github.com/inflatable-cookie/effigy/releases/tag/v0.2.5`
- migration notes summary:
  - built-in release orchestration shipped as the primary operator path
  - release-note extraction in the workflow was moved onto built-in
    `effigy changelog extract`
  - release wrappers remained documented as backup channels only

## Release Wrapper Retirement Record

- Release evaluated: `v0.2.5`
- Prior built-in release in comparison window: none; this is the first
  production built-in release
- Built-in release path used for this release:
  - [x] `effigy release prepare`
  - [x] `effigy release execute`
- Wrapper fallback used:
  - [x] no
  - [ ] yes, with explanation recorded
- Hosted release workflows green for this release:
  - [x] `CI`
  - [x] `JSON Contracts`
  - [x] `Release Binaries`
- Tag install validation green in this release window:
  - [x] production release path and built-in verify-install surface remained
        aligned
- Any active CI/docs/downstream contract still points to wrapper scripts:
  - [ ] no
  - [x] yes, wrapper scripts are still documented as backup channels and remain
        part of compatibility policy
- Maintainer decision:
  - [x] keep wrappers for another release cycle
  - [ ] retire `scripts/prepare-release.sh`
  - [ ] retire `scripts/check-release-gates.sh`
  - [ ] retire `scripts/check-release-install-from-tag.sh`
- Decision owner: effigy maintainers
- Notes:
  - Wrapper retirement criteria are not yet met because the comparison window
    requires two consecutive real built-in releases.
  - Distribution first-publish evidence was not standardized into this
    checkpoint artifact, so channel-closeout confidence is still incomplete.

## Risks / Follow-ups

- Remaining release risks:
  - wrapper-retirement evidence window is incomplete after only one production
    built-in release
  - first-publish artifact capture and closeout evidence should be attached in
    the next comparable release window
- Follow-up tasks:
  - use guide `054` for the next real release checkpoint log
  - fill out the wrapper-retirement comparison against the next production
    built-in release
  - decide whether release backup wrappers remain necessary after that second
    comparison point

## Next

- For the next real Effigy release, capture the full distribution artifact
  bundle and complete the second wrapper-retirement comparison window so the
  maintainers can make an actual keep-or-retire decision for the three release
  compatibility wrappers.
