# 039 - Pre-Release CI Proof Contract

Status: active
Owner: Platform

## Purpose

Effigy must not create a release commit or tag from source that has not passed
the repository's hosted CI board. Local release gates are additional evidence,
not a substitute for CI.

## Invariant

Before release simulation, gate-checked status, prepare, or execute:

- the working source is a clean commit already pushed to `main`
- `ci.yml` was manually dispatched for that commit
- the completed run has `event = workflow_dispatch`, `headBranch = main`, the
  exact candidate `headSha`, and `conclusion = success`
- missing, queued, running, red, cancelled, stale, or different-SHA evidence
  blocks the release

"Latest green main" is not sufficient. Evidence is bound to the candidate SHA.

## Mutation Boundary

Hosted CI validates the source commit. Prepare may then make only the
deterministic version, changelog, coordinated path-dependency, lockfile, and
prepared-state mutations already governed by the release contracts. Local
release gates validate those mutations before execute. Prepared-source
fingerprints and branch checks prevent unrelated drift between CI proof and
tag creation.

## Ownership

- `.github/workflows/ci.yml` owns the hosted board and manual trigger
- `scripts/check-release-ci.sh` owns exact-SHA evidence lookup for this repo
- `config/release.toml` makes that lookup a required release gate
- release guides and the bundled agent skill own dispatch/watch sequencing
- the generic Effigy release engine remains provider-neutral; consumers supply
  an equivalent gate for their CI provider

## Validation

- the checker passes only when `gh run list` returns the current `HEAD`
- a missing or different SHA fails with direct dispatch remediation
- self-hosted release config includes the `ci` gate
- both skill copies and active release guides put hosted CI before release
  preview, prepare, and execute

## Drift Triggers

- a release sequence begins with local preview or gates before hosted CI
- a check accepts a run from another commit or trigger
- CI becomes advisory instead of release-blocking
- workflow behavior changes without matching checker and protocol review

## Next Task

Maintain the exact-SHA invariant. No release is implied by this contract.
