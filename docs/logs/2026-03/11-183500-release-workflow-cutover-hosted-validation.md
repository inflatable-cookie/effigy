# Release Workflow Cutover Hosted Validation

Date: 2026-03-11
Roadmap: g01.027
Batch: release-workflow-cutover-hosted-validation

## Summary

Applied the approved `.github/workflows/` release-workflow cutover in the main
repo, then validated it end to end on the hosted rehearsal repo
`inflatable-cookie/effigy-rehearsal`.

Validated changes:

- `.github/workflows/release-binaries.yml` now uses built-in
  `effigy changelog extract` for release-note extraction
- release workflow keeps the generated-notes fallback when extraction fails or
  returns an empty body
- `actions/checkout`, `actions/setup-node`, `actions/upload-artifact`, and
  `actions/download-artifact` were refreshed to current major tags in the
  touched workflows

## Hosted Validation Performed

### 1. PR validation on workflow commit

Updated rehearsal PR branch:

- branch: `ci-rerun-20260311-175822`
- PR: `https://github.com/inflatable-cookie/effigy-rehearsal/pull/1`

Results on the workflow-cutover commit:

- `CI`: success
- `JSON Contracts`: success

Observed warning after the action refresh:

- remaining Node runtime deprecation warning comes from `Swatinem/rust-cache@v2`
  only

This means the GitHub-maintained actions refreshed in this batch no longer
produced the earlier Node 20 warnings during hosted validation.

### 2. Tag-triggered release workflow

Ran built-in release flow from the same validated branch:

```text
effigy release prepare.. --yes --check-gates --version 0.2.7
effigy release execute.. --yes
```

Results:

- branch push succeeded
- tag `v0.2.7` succeeded
- `Release Binaries` workflow run succeeded:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22967881623`

### 3. Release-note extraction proof

The workflow completed the new release-job sequence successfully:

- `Build effigy`: success
- `Extract release notes from CHANGELOG.md`: success
- `Create release`: success

Published hosted rehearsal release:

- `https://github.com/inflatable-cookie/effigy-rehearsal/releases/tag/v0.2.7`

Verification outcome:

- the release body contains the expected changelog section for `0.2.7`
- it did not fall back to GitHub-generated notes
- the release body includes the new workflow-cutover changelog entry, proving
  the workflow consumed the changelog content from the tagged repo state

## Current Assessment

Workflow cutover from legacy `sed` extraction to built-in
`effigy changelog extract` is now implemented and hosted-validated.

What remains outside this batch:

- first live production Effigy release through the built-in workflow
- eventual wrapper retirement after human confidence is established
- optional future replacement of `Swatinem/rust-cache@v2` if upstream Node 24
  support becomes available or if the project decides to replace that action

## Evidence

- PR `CI` success:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22967684909`
- PR `JSON Contracts` success:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22967684943`
- release workflow success:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22967881623`
- hosted release:
  `https://github.com/inflatable-cookie/effigy-rehearsal/releases/tag/v0.2.7`

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Moved from: workflow cutover approved in principle but only documented as a
  review-only diff
- Moved to: workflow cutover implemented in repo and validated on hosted tag
  release infrastructure with real release-note extraction
- Remaining open:
  - first live production Effigy release through built-in workflow
  - wrapper retirement decision
  - rust-cache Node 24 warning remains upstream-facing hygiene work
