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

## A Hosted Gate Is A Repo Task

Effigy core knows no provider. The `ci` proof is a repo task wired into
`[release.gates]` — `ci = { task = "release:ci-evidence" }` in
`config/release.toml`, the task in `config/tasks.toml` — and every consumer
declares its own the same way. The runner exports the exact commit the gate
must prove as `EFFIGY_RELEASE_COMMIT`: the source commit at `pre-gates`, the
prepared candidate at `certify`. One gate task, one truth, wherever it runs:
`release gates`, status with `--check-gates`, and a Run.

- accept only this repository's GitHub Actions runs, queried through
  authenticated `gh` with `--repo owner/name` taken from `origin`
- require the commit the runner named; an identical tree at another SHA fails
- refuse zero or several `workflow_dispatch` runs for that commit rather than
  taking the newest, because a sibling dispatch or re-run is a different run
- missing, pending, failed, ambiguous, wrong-repository, or wrong-SHA evidence
  exits non-zero, so the gate stage fails and nothing publishes behind it

The existing `ci` proof remains a prerequisite, not permission to skip other
gates.

## Ownership

- `.github/workflows/ci.yml` owns the hosted board and manual trigger
- `release:ci-evidence` (`config/tasks.toml`) is this repository's `ci` gate
  task; `scripts/check-release-ci.sh` owns its exact-SHA evidence lookup and
  refuses zero or several `workflow_dispatch` runs for the commit rather than
  taking the newest
- the release engine owns `EFFIGY_RELEASE_COMMIT`, the commit each gate stage
  must prove, and nothing provider-specific
- `config/release.toml` makes the `ci` task a required release gate
- release guides and the bundled agent skill own dispatch/watch sequencing

## Validation

- the checker passes only when `gh run list` returns the current `HEAD`
- a missing or different SHA fails with direct dispatch remediation
- self-hosted release config includes the `ci` gate
- both skill copies and active release guides put hosted CI before release
  preview, prepare, and execute
- the `ci` gate task fails closed on missing, pending, failed, ambiguous,
  wrong-repository, or wrong-SHA evidence
- status with `--check-gates`, standalone `release gates` and a Run report the
  same gate outcomes

## Drift Triggers

- a release sequence begins with local preview or gates before hosted CI
- a check accepts a run from another commit, repository, or trigger
- CI becomes advisory instead of release-blocking
- named reuse silently falls back to local execution when evidence is bad
- workflow behavior changes without matching checker and protocol review
