# 005 Optional Distribution Surface Strict Lane

Status: paused
Updated: 2026-04-15
Roadmap: `g02.005`

## Context

Effigy now has native distribution commands that replace more of the old
release/distribution script layer. That internal proof is good enough to stop
treating the question as a Rhai-only dogfooding lane.

The next product-shaping problem is optional cross-repo reuse: how Effigy
should expose distribution tooling so other repos can choose to use it without
being forced into Effigy's exact release model.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/005-optional-distribution-surface-contract.md`

## Lane Focus

The active strict lane is:

- define the optional manifest-driven distribution boundary
- separate reusable distribution primitives from Effigy-self-hosting policy
- document the distribution surface as a product front door
- implement only the minimal foundation needed for cross-repo adoption

## Current Posture

`strict-paused`

The Rhai lane is paused on a clean internal boundary. Native distribution
cutover is shipped strongly enough that the next valid move is optional
distribution productization, not more scripting churn.

The first manifest-driven foundation is now shipped for package identity,
preflight tasks, and metadata requirements.

That widening batch shipped too:

- publish identity can be manifest-driven
- summary identity can be manifest-driven
- closeout defaults can be manifest-driven

That consumer proof shipped too:

- `convergence` adopted minimal `[distribution]` package/publish/closeout
  policy
- `distribution validate-artifacts` passed against real consumer proof logs
- `distribution generate-closeout` produced a repo-owned closeout cleanly
- `distribution validate-metadata` still failed on Effigy-specific workflow
  assumptions
- the fuller `distribution first-publish` path still assumes an
  Effigy-compatible CLI self-inspection shape

That widening batch is now shipped too:

- manifest-adopting repos no longer inherit Effigy's workflow/docs/package
  metadata checks by default in `distribution validate-metadata`
- manifest-adopting repos can disable `verify-tag-install` and
  `verify-binary-json-tasks` in `[distribution.publish]`
- the `convergence` proof now passes `validate-metadata`,
  `validate-artifacts`, and `generate-closeout` against the widened contract

That decision is now settled too:

- the current optional boundary is strong enough to pause credibly
- metadata validation, artifact validation, and closeout evidence are proven in
  a real consumer repo
- the remaining full `first-publish` question is now an explicit deferred
  published-consumer limit, not a hidden product gap

The lane is therefore paused on a trustworthy boundary instead of forcing one
more proof batch just to chase a narrower published-install workflow question.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the distribution question broadens, stop and ask whether the priority is:

- reusable built-ins
- manifest contract design
- or documentation/adoption guidance

Do not guess.

## Exit Condition

This strict lane is complete when Effigy has a documented, optional
distribution surface that other repos can adopt without inheriting Effigy's
hardcoded release policy.

## Next Task

Keep `g02.005` paused on the current optional distribution boundary until a
real published-consumer need justifies reopening the fuller `first-publish`
question.
