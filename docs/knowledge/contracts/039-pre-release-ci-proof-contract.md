# 039 - Pre-Release CI Proof Contract

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
prepared-state mutations already governed by the release contracts. Those
mutations do not create a new commit; `HEAD` remains the source SHA.

Named hosted reuse is bound to that `HEAD` SHA. It attests the source commit,
not the prepared working tree. Gates not named for reuse still run locally,
including against the prepared files before execute. Naming a mutation-sensitive
gate for reuse is a consumer decision. Prepared-source fingerprints and branch
checks prevent unrelated drift between CI proof and tag creation.

## Named Hosted Reuse

A gate may opt in with `reuse-hosted-evidence = true` under `[release.gates]`
when `[release.hosted-evidence].workflow` is set. Status, prepare, simulate,
standalone `release gates`, and execute then share the same gate truth:

- accept only this repository's GitHub Actions runs, queried through
  authenticated `gh` with `--repo owner/name` taken from `origin`
- require the exact release commit SHA; an identical tree at another SHA fails
- reuse only the named gates; every configured gate stays in the policy
- record each reused gate and its run URL in gate JSON, logs, and
  `.release-prepared.json`
- missing, pending, failed, ambiguous, wrong-repository, or wrong-SHA evidence
  fails closed with a diagnostic; the named gate does not fall back to local
  execution

The existing `ci` proof remains a prerequisite, not permission to skip other
gates. Hosted reuse is an opt-in GitHub Actions path. Consumers on other
providers keep supplying local gates.

## Ownership

- `.github/workflows/ci.yml` owns the hosted board and manual trigger
- `scripts/check-release-ci.sh` owns exact-SHA evidence lookup for this repo's
  required `ci` gate
- the release engine owns named reuse lookup, SHA and repository checks, and
  persisted run links
- `config/release.toml` makes the `ci` lookup a required release gate
- release guides and the bundled agent skill own dispatch/watch sequencing

## Validation

- the checker passes only when `gh run list` returns the current `HEAD`
- a missing or different SHA fails with direct dispatch remediation
- self-hosted release config includes the `ci` gate
- both skill copies and active release guides put hosted CI before release
  preview, prepare, and execute
- named reuse fails closed on missing, pending, failed, ambiguous,
  wrong-repository, or wrong-SHA evidence
- status, prepare, and execute report the same reused gates and run links

## Drift Triggers

- a release sequence begins with local preview or gates before hosted CI
- a check accepts a run from another commit, repository, or trigger
- CI becomes advisory instead of release-blocking
- named reuse silently falls back to local execution when evidence is bad
- workflow behavior changes without matching checker and protocol review
