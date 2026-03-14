# Release Cutover Readiness Rehearsal Brief

Date: 2026-03-11
Repo: `inflatable-cookie/effigy` and hosted rehearsal repo `inflatable-cookie/effigy-rehearsal`

## Summary

The built-in release flow is now validated through both a zero-risk local
rehearsal and a hosted GitHub rehearsal.

Validated command path:

```text
effigy release simulate..
effigy release status..
effigy release prepare.. --yes --check-gates
effigy release execute.. --plan
effigy release execute.. --yes
```

Validated release variants:

- local disposable clone + local bare remote
- hosted throwaway repo + real GitHub tag-triggered workflow
- hosted custom version override path (`--version 0.2.6`)

## Proven In This Rehearsal

- `release simulate` passes against Effigy's real configured gate stack.
- `release prepare --yes --check-gates` passes on a version-bumped release
  tree, including `build`, `format`, `metadata`, `qa`, `smoke`, and `test`.
- `release execute --yes` commits, tags, pushes, and removes
  `.release-prepared.json`.
- GitHub tag push triggers `.github/workflows/release-binaries.yml`.
- Hosted `Release Binaries` completed successfully for `v0.2.5`.
- Hosted `CI` and `JSON Contracts` both completed successfully for the
  post-fix rehearsal PR `inflatable-cookie/effigy-rehearsal#1`.
- Release-note extraction remains functionally valid in the current workflow,
  but still uses the legacy `sed` path rather than built-in
  `effigy changelog extract`.

## Issues Found During Rehearsal

The rehearsal found real gaps and resolved them:

1. `scripts/check-distribution-metadata.sh` still referenced obsolete workflow
   files and helper scripts. The metadata gate was updated to assert against the
   current release workflow wiring.
2. Changelog tests assumed `[Unreleased]` must always contain entries. That is
   false immediately after a legitimate release preparation, so tests were
   updated to accept both pre-release and freshly prepared states.
3. Hosted `CI` exposed two clippy violations in
   `src/runner/release_command.rs` that were not covered by the built-in
   release gates. Those violations were fixed and verified.

## Current Cutover Assessment

The built-in release flow is now ready for human-reviewed production adoption,
but not yet for an unconditional cutover.

Why it is now substantially lower risk:

- the command surface is no longer only locally tested
- the built-in flow has now created and pushed real rehearsal tags
- the real GitHub release workflow has run successfully from those tags
- hosted `CI` parity for the release-command changes has been revalidated

Why it is still not a full cutover:

- the real Effigy production repo has not yet been released through the built-in
  path
- `.github/workflows/release-binaries.yml` still uses the legacy `sed` release
  note extraction path
- wrapper retirement and operator cutover are still human policy decisions
- the Node 20 deprecation warnings in GitHub Actions remain repo hygiene work,
  though they were warnings only in rehearsal

## Recommended Next Decision

Safe next step:

- use this evidence to approve a first human-supervised live Effigy release via
  built-in `effigy release ...` while keeping wrappers available as backups

Still defer:

- workflow cutover from `sed` to `effigy changelog extract`
- wrapper retirement
- any claim that production releases are fully routine or hands-off

## Evidence Pointers

- Hosted rehearsal PR: `https://github.com/inflatable-cookie/effigy-rehearsal/pull/1`
- Hosted release workflow (`v0.2.5`) completed:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22966224918`
- Hosted PR CI completed:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22967000848`
- Hosted PR JSON contracts completed:
  `https://github.com/inflatable-cookie/effigy-rehearsal/actions/runs/22967000823`

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Moved from: built-in release flow shipped but only regression-tested and
  locally rehearsed
- Moved to: built-in release flow exercised through real hosted tag/release
  automation with CI parity revalidated
- Remaining open:
  - first live production Effigy release through the built-in path
  - approved workflow cutover from `sed` to `effigy changelog extract`
  - wrapper retirement after live-release confidence
