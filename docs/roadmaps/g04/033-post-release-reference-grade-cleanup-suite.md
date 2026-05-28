# 033 - Post-Release Reference-Grade Cleanup Suite

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`032-example-app-deployment-proof-and-closeout.md`](./032-example-app-deployment-proof-and-closeout.md)

## Goal

Turn the post-v0.6.x codebase sweep into an ordered cleanup suite that improves
ownership, modularity, and explainability without starting speculative rewrites.

## Scope

- preserve the audit evidence behind the next cleanup lanes
- sequence the cleanup work by risk and leverage
- keep implementation work behind strict roadmap and batch-card gates
- prefer staged extraction over broad rewrites
- keep public behavior stable unless a later roadmap explicitly chooses a
  breaking cleanup

Primary evidence:

- `effigy scan god-files --json`
- `effigy scan duplicate-blocks --json`
- `effigy scan comment-ratio --json`
- `effigy scan attention-markers --json`
- `effigy test --plan`
- manual inspection of runner, manifest, deploy, state, artifact, CLI help,
  docs-policy, and crate-boundary surfaces

## Ordered Follow-Up Lanes

1. [`034-shared-database-target-resolution.md`](./034-shared-database-target-resolution.md)
2. [`035-state-domain-extraction.md`](./035-state-domain-extraction.md)
3. [`036-manifest-section-decomposition.md`](./036-manifest-section-decomposition.md)
4. [`037-deploy-domain-boundary-hardening.md`](./037-deploy-domain-boundary-hardening.md)
5. [`038-docs-policy-cli-help-and-test-fixture-deduplication.md`](./038-docs-policy-cli-help-and-test-fixture-deduplication.md)
6. [`039-artifact-and-crate-boundary-rejustification.md`](./039-artifact-and-crate-boundary-rejustification.md)

## Non-Goals

- no release execution
- no `.github/workflows/` edits
- no broad rewrite of runner orchestration
- no crate merge or split without concrete ownership evidence
- no abstraction unless there are at least two concrete call sites or a clear
  domain boundary
- no user-facing command changes unless a later roadmap explicitly scopes them

## Acceptance Criteria

- the cleanup suite is linked from the g04 index
- each follow-up roadmap names its target evidence and success criteria
- the first execution move is explicit
- implementation batch cards are deferred until each lane opens
- the suite keeps the Example App migration/deployment path in view, especially
  shared data target resolution and state extraction

## Validation

- `git diff --check`
- docs review
- later implementation lanes choose focused test commands before code changes

## Outcome

- the post-v0.6.x cleanup suite is sequenced through `g04.034` to `g04.039`
- shared database target resolution is selected as the first active lane
- implementation remains gated by strict batch cards

## Next Task

Open shared database target resolution. It removes a real split path before
state, media, and Example App migration work build on it.
