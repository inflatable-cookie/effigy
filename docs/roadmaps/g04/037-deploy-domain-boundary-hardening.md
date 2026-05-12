# 037 - Deploy Domain Boundary Hardening

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`036-manifest-section-decomposition.md`](./036-manifest-section-decomposition.md)

## Goal

Reduce deploy runner ownership by separating transaction models, report
persistence, provider-package dispatch, and rendering boundaries.

## Evidence

- `src/runner/deploy_command/transaction.rs` is 1,256 lines
- `src/runner/deploy_command/provider_package.rs` is 550 lines
- v0.6.x moved live provider logic into provider packages, but core still owns
  too much deploy transaction glue in runner code
- Render and Railway package work will continue to evolve outside Effigy core,
  so the core boundary needs to stay narrow and stable

## Scope

- classify deploy transaction code into domain model, persistence, provider
  package adapter, command dispatch, and text rendering
- promote stable transaction/report models into a deploy domain module or crate
- isolate provider-package execution context from command rendering
- keep `deploy export` separate from live `deploy apply`
- keep provider-specific live behavior outside Effigy core
- preserve existing JSON schemas

## Non-Goals

- no provider resource provisioning
- no provider secret management
- no new deploy command surface
- no Render or Railway package implementation changes
- no database rollback
- no release command execution from deploy

## Boundary Decision

Implementation should decide whether a new `effigy-deploy` crate is justified
or whether an internal runner/domain split is enough for this tranche.

Do not create a crate only to reduce file size. Create it only if deploy has a
stable domain API that other code can depend on without pulling runner concerns.

## Acceptance Criteria

- deploy plan/apply/status/history/redeploy models have clear ownership
- provider-package dispatch is isolated behind a small adapter boundary
- report path and history helpers are not tangled with command text rendering
- JSON contract tests remain stable across the split
- built-in provider export remains compatible

## Outcome

- split deploy report schemas and persistence into `report.rs`
- split provider context assembly and policy blockers into `provider_context.rs`
- split text rendering into `text.rs`
- kept provider package execution in `provider_package.rs`
- kept static deploy model/export behavior unchanged
- reduced `transaction.rs` from 1,256 lines to 856 lines

## Suggested Batch Cards

- `674-open-deploy-domain-boundary-lane.md`
- `675-classify-deploy-transaction-ownership.md`
- `676-extract-deploy-report-models-and-history-helpers.md`
- `677-isolate-provider-package-dispatch-context.md`
- `678-split-deploy-text-rendering-from-transaction-state.md`
- `679-close-deploy-boundary-proof.md`

## Validation

- deploy command tests
- JSON contract examples for plan/apply/status/history
- provider package fixture tests
- `effigy deploy plan <fixture> --json`
- `effigy deploy status <fixture> --json`
- `git diff --check`

## Next Task

Execute `038` for docs-policy, CLI help, and fixture deduplication.
