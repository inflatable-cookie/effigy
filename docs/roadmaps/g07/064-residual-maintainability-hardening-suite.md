# g07.064 - Residual Maintainability Hardening Suite

Status: Planned
Depends on: `g07.063`

## Goal

Turn the explicit residual debt from the `g07` closeout into one bounded,
evidence-driven cleanup sequence.

## Why This Exists

The `g07` closeout was honest:

- `effigy scan god-files --json` still reports `3` warning-only files
- `effigy scan duplicate-blocks --json` still reports `7` high findings
- runner-private fixture duplication and graph-test sprawl still cost time
  during review and proof work

That is not emergency product debt. It is the next maintainability tax.

## Scope

- reduce the remaining god-file set where the boundary is obvious
- remove or justify the remaining high duplicate-block findings
- narrow graph test support so extractor/query changes are easier to validate
- reduce runner-private fixture duplication only where ownership is clear
- keep docs and planning state aligned with the reopened generation

## Non-Goals

- no new user-facing graph feature lane
- no release or workflow work
- no general rewrite of the runner
- no scan-score vanity work

## Ordered Follow-Up Lanes

1. [`065-manifest-semantic-owner-split.md`](./065-manifest-semantic-owner-split.md)
2. [`066-codegraph-test-harness-decomposition.md`](./066-codegraph-test-harness-decomposition.md)
3. [`067-script-command-boundary-reduction.md`](./067-script-command-boundary-reduction.md)
4. [`068-high-duplicate-help-fragment-reduction.md`](./068-high-duplicate-help-fragment-reduction.md)
5. [`069-language-emitter-follow-through.md`](./069-language-emitter-follow-through.md)
6. [`070-runner-private-fixture-and-helper-convergence.md`](./070-runner-private-fixture-and-helper-convergence.md)
7. [`071-residual-maintainability-closeout.md`](./071-residual-maintainability-closeout.md)

## Acceptance Criteria

- every remaining `g07` residual cluster is either reduced, justified, or
  deliberately deferred
- no public contract drift lands without focused proof
- the generation closes with an updated scan baseline and explicit residuals

## Next Task

Plan and execute `g07.065`.
