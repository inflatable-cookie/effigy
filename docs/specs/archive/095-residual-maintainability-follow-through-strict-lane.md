# 095 - Residual Maintainability Follow Through Strict Lane

Roadmap: [`g07.064`](../roadmaps/g07/064-residual-maintainability-hardening-suite.md)
Related planning:
- [`g07.065`](../roadmaps/g07/065-manifest-semantic-owner-split.md)
- [`g07.066`](../roadmaps/g07/066-codegraph-test-harness-decomposition.md)
- [`g07.067`](../roadmaps/g07/067-script-command-boundary-reduction.md)
- [`g07.068`](../roadmaps/g07/068-high-duplicate-help-fragment-reduction.md)
- [`g07.069`](../roadmaps/g07/069-language-emitter-follow-through.md)
- [`g07.070`](../roadmaps/g07/070-runner-private-fixture-and-helper-convergence.md)
- [`g07.071`](../roadmaps/g07/071-residual-maintainability-closeout.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Execute the residual maintainability follow-through from the `g07` closeout
without turning warning-only debt into another broad rewrite.

## Lane Posture

Posture: `complete`

This lane exists because `g07.063` closed honestly with three warning-only
god-file findings, seven high duplicate-block findings, and a few deferred
runner/help/test-support clusters that still tax future work.

The work remains maintenance with product discipline: behavior stays stable
unless focused proof shows the current behavior is wrong.

## Hard Boundaries

- no release mutations
- no `.github/workflows/` edits
- no graph storage or public JSON rewrite
- no new graph feature work hidden inside cleanup
- no generic multi-language extraction framework
- no runner rewrite
- no crate merge without explicit evidence and a separate decision

## Execution Order

1. `1014`: open the lane and lock the current residual baseline
2. `1015`: split manifest semantic ownership
3. `1016`: decompose the codegraph test harness
4. `1017`: reduce script-command owner sprawl
5. `1018`: trim the remaining high help-topic duplicate clusters
6. `1019`: inspect and reduce the remaining high language-emitter duplicates
7. `1020`: converge runner-private fixture/helper duplication
8. `1021`: close with proof and residual debt

## Ready Chain

- `1014` is complete
- `1015` is complete
- `1016` is complete
- `1017` is complete
- `1018` is complete
- `1019` is complete
- `1020` is complete
- `1021` is complete
- later cards must not start until the prior card is complete or explicitly
  paused with a clear handoff

## Auto-Continuation Envelope

Auto-start is enabled while:

- work follows the ordered cards
- public CLI and JSON behavior stays stable
- cleanup remains local to the named surface
- focused validation is run before moving to the next surface

Stop and replan if:

- a cleanup requires public contract changes
- a helper extraction starts hiding ownership instead of clarifying it
- graph proof depth would need to shrink to make the file split work
- runner extraction starts crossing shell/process glue into domain crates

## Acceptance

This lane is complete when:

- all cards `1014` through `1021` are complete
- scan deltas and tests are recorded
- remaining debt is explicitly deferred, justified, or rejected
- no active ready card remains

## Next Task

No active ready card.
