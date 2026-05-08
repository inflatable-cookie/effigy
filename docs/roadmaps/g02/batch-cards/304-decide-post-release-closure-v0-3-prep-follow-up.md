# 304 Decide Post Release-Closure v0.3 Prep Follow-Up

Status: archived
Updated: 2026-04-20
Roadmap: `g02.007`
Spec: `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`

## Objective

Choose the next bounded `g02.007` move now that `115` is complete but the live
repo state no longer matches the older `v0.2.14` release-closure evidence.

## Scope

- reassess the release lane against current built-in release-command output
- decide whether the next bounded move is direct release execution or one more
  release-prep alignment batch for the deliberate `v0.3.0` cut
- refresh the front-door planning surfaces so `continue` resolves through the
  real blocker set instead of stale `115` language

## Out Of Scope

- running irreversible release commands
- broader consumer rollout planning
- unrelated product execution outside release-prep alignment

## Acceptance

- one explicit next execution card exists for `g02.007`
- the lane front doors stop implying `115` is still the active ready card
- the chosen batch matches the actual `v0.3.0` release posture

## Decision

The next `g02.007` batch should be one bounded `v0.3.0` release-prep alignment
slice, not direct release execution.

Why one more prep slice comes first:

- the live built-in release flow still suggests `0.2.14` by default, while the
  active roadmap and release protocol now say the intended cut is `v0.3.0`
- `effigy release simulate` and `release status --check-gates` are both
  blocked today by the `format` gate, so the repo is not currently in a clean
  ready-to-prepare state anyway
- the worktree also carries active non-release edits, which makes direct
  release execution the wrong operator move even before human approval rules
- `115` proved the release surface and broader closure posture. What remains is
  to realign the lane on the actual target version and current gate state, not
  to reopen product hardening or improvise a release from stale evidence

What stays out of the next batch:

- `release prepare --yes`
- `release execute --yes`
- tag push or hosted release monitoring

## Result

The next explicit `g02.007` execution batch is now card `305`.

## Next Task

Execute `305` to align the release lane on the deliberate `v0.3.0` target and
leave one honest pre-release checkpoint.
