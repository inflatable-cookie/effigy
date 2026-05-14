# g05.008 - Post-Release Reference-Grade Follow-Through Suite

Status: Complete
Depends on: `g05.007`

## Goal

Turn the latest post-release codebase sweep into an ordered cleanup queue that
finishes ownership moves already implied by current contracts instead of opening
another broad architecture generation.

## Scope

- reopen `g05` for post-release structural follow-through
- queue the highest-leverage cleanup lanes from the audit
- keep the work ordered by ownership and dependency, not by annoyance
- preserve public behavior unless a later roadmap explicitly widens scope
- keep implementation behind bounded batch cards and targeted validation

## Ordered Follow-Up Lanes

1. [`009-state-command-thin-shell-follow-through.md`](./009-state-command-thin-shell-follow-through.md)
2. [`010-shared-secrets-vault-access-boundary.md`](./010-shared-secrets-vault-access-boundary.md)
3. [`011-container-lifecycle-owner-split.md`](./011-container-lifecycle-owner-split.md)
4. [`012-rhai-internal-boundary-follow-through.md`](./012-rhai-internal-boundary-follow-through.md)
5. [`013-cli-help-topic-descriptor-convergence.md`](./013-cli-help-topic-descriptor-convergence.md)
6. [`014-area-local-test-builder-cleanup.md`](./014-area-local-test-builder-cleanup.md)
7. [`015-active-docs-reference-refresh-and-g05-closeout.md`](./015-active-docs-reference-refresh-and-g05-closeout.md)

## Evidence

- `docs/audits/reusable-codebase-sweep-prompt.md`
- latest codebase sweep audit report
- `effigy scan god-files --json`
- `effigy scan duplicate-blocks --json`
- `effigy scan comment-ratio --json`
- `effigy scan attention-markers --json`
- `effigy test --plan`
- current `g04` and `g05` contracts and completed cleanup lanes

## Non-Goals

- no release execution
- no `.github/workflows/` edits
- no speculative new crates
- no broad rewrite of runner orchestration
- no command grammar redesign

## Acceptance Criteria

- the reopened `g05` queue is documented in roadmap front doors
- each actionable audit job has a bounded roadmap file
- the queue order reflects dependency and leverage
- no implementation starts until a bounded batch card opens the selected lane

## Outcome

- reopened `g05` execution under strict lane `081`
- completed the state thin-shell, shared vault-access, container lifecycle,
  Rhai internal-boundary, help-registry, local fixture-builder, duplicate-proof,
  and active-doc-reference slices
- closed the reopened cleanup suite explicitly with residual blockers and
  deferrals recorded rather than hidden

## Next Task

No next task. The reopened `g05` cleanup suite is closed.
