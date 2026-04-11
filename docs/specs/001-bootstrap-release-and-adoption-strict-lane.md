# 001 Bootstrap Release And Adoption Strict Lane

Status: complete
Updated: 2026-04-11
Roadmap: `g02.001`

## Context

Effigy already has a coherent active roadmap lane in `g02.001`, and the repo
worktree is clean. What it lacks is the stricter execution grammar now being
used elsewhere in Northstar projects: explicit ready cards, currentness
surfaces that advertise the real next batch, and clear stop rules when the
work becomes ambiguous.

This spec wraps the active bootstrap/release/adoption lane in that stricter
execution grammar without changing the product scope.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/001-bootstrap-command-and-clone-contract.md`

## Lane Focus

The active strict lane is:

- finish the honest next step for `g02.001`
- keep the decision bounded between:
  - one more bootstrap proof wave if release confidence is still too weak
  - release-preparation work if the live pilots already prove the feature well
    enough

This spec does not reopen broader research, product-boundary cleanup, or
unrelated built-in expansion.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the evidence from the live pilots does not make the next move obvious, stop
and ask whether the priority is:

- release confidence and gating
- one more workspace/bootstrap proof wave
- a narrower product-boundary clarification batch

Do not guess.

## Exit Condition

This strict lane is complete when `g02.001` no longer relies on roadmap text
alone for the next move and the current ready batch is explicit from the front
doors.

## Next Task

This lane is complete. Use `g02.002` as the active strict lane for manifest
composition and override planning.
