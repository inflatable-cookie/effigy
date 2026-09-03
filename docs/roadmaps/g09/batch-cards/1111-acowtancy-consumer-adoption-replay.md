# 1111 - Acowtancy Consumer Adoption Replay

Roadmap: [`../003-acowtancy-consumer-adoption-replay.md`](../003-acowtancy-consumer-adoption-replay.md)
Spec: [`../../../specs/118-acowtancy-consumer-adoption-replay-strict-lane.md`](../../../specs/118-acowtancy-consumer-adoption-replay-strict-lane.md)
Decision: [`D-2026-05`](../../../vision/decisions/D-2026-05-consumer-adoption-cohort-replay.md)

Status: Ready
Owner: frozen consumer replay, evidence ownership, comparison scorecard, and
bounded Effigy starter/guide reconciliation
Created: 2026-09-03

## Purpose

Prove the current consumer contract against Acowtancy at an exact revision and
turn the result into governed cross-repository evidence.

## Acceptance

- record Effigy `main` SHA, built binary identity, Acowtancy repository URL,
  and frozen Acowtancy SHA
  `e42b64b17cae15ed419872ccb9bdfc48861d5214`
- prove the consumer worktree is clean and exactly at the frozen SHA before and
  after the replay; do not fetch, merge, checkout, reset, clean, or edit there
- inventory selector ownership, then run only read-only/non-starting surfaces:
  `effigy tasks`, `effigy doctor`, `effigy test --plan`,
  `effigy docs/qa:docs`, and `effigy docs/qa:northstar`
- capture text or JSON outcomes, exit status, selected root/catalog/task, and
  remediation for every failure; distinguish consumer policy, Effigy behavior,
  environment, and intentional repository variation
- explicitly inspect the retained child-catalog/container-registry workaround;
  record it as retained unless Acowtancy separately supplies downstream
  revalidation—this card never removes or edits it
- create a populated scorecard under `docs/vision/governance/` comparing Effigy
  and Acowtancy across `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`, and `RELEASE`;
  every score needs a direct evidence link and unknowns stay unknown
- update guide `056`, the Northstar starter, or related Effigy guidance only if
  the frozen replay proves a generic mismatch; add proportional recurrence
  proof for any changed machine-owned starter surface
- write one dated Effigy evidence log mapping the replay and every review-oracle
  row; update card/roadmap/spec/front doors with the honest continuation state

## Validation

- Acowtancy pre/post `git status --porcelain`, `git rev-parse HEAD`, root
  resolution, and selector inventory evidence
- the five bounded consumer commands named above, run without containers,
  secrets, installs, state mutation, or managed sessions
- scorecard evidence-link and no-unsupported-score review
- focused tests for any changed Effigy starter behavior; otherwise docs-only
  validation
- `effigy qa:docs`, `git diff --check`, and `effigy doctor --json` in Effigy
- additional Effigy tests only as required by the actual changed surface

## Evidence

Write one log under `docs/logs/2026-09/` containing the frozen identities,
command matrix, failure ownership, pre/post consumer state, retained-workaround
result, scorecard rationale, changed Effigy surfaces, validation, and next
cohort recommendation. Do not write evidence into Acowtancy.

## Review Oracle

Reject the PR if:

1. either repository identity is missing, mutable, or differs from the frozen
   replay boundary;
2. Acowtancy changes in any way or a command starts runtime/stateful work;
3. selector/root ownership is not proved before attributing a result;
4. any failure lacks an owner and next action, or consumer policy is mislabeled
   as an Effigy defect;
5. a score lacks linked evidence, an unknown is scored, or one pilot supports a
   universal claim;
6. an Effigy edit is not the smallest generic repair directly required by the
   replay;
7. the retained workaround is edited or pronounced obsolete without separate
   Acowtancy-owned downstream revalidation;
8. the command matrix omits tasks, doctor, test plan, docs QA, or Northstar QA.

## Stop Conditions

Stop on a dirty or moved consumer checkout, any need to modify Acowtancy, any
runtime/container/secret/install/state prerequisite, ambiguous selector
ownership, an Effigy product-code change, workflow/release mutation, a second
consumer, S3/provider scope, or evidence that changes the portable contract's
meaning rather than documenting a bounded mismatch.

## Next Task

Run this card in one Effigy worker lane. Return the exact-head PR to the
orchestrator; do not merge.
