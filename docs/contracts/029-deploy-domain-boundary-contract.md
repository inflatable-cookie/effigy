# Deploy Domain Boundary Contract

Generation: `g04`
Roadmap: [`../roadmaps/g04/037-deploy-domain-boundary-hardening.md`](../roadmaps/g04/037-deploy-domain-boundary-hardening.md)
Strict lane: [`../specs/073-deploy-domain-boundary-hardening-strict-lane.md`](../specs/073-deploy-domain-boundary-hardening-strict-lane.md)
Status: Accepted
Owner: Platform
Updated: 2026-05-12

## Purpose

Define the structural boundary for hardening Effigy's deploy transaction
implementation after v0.6.x.

The deployment surface is now large enough that command runners should not own
every model, persistence helper, provider context, and renderer in one file.

## Hard Boundaries

- no deploy command grammar changes
- no deploy JSON schema changes unless a later card explicitly scopes a
  compatible addition
- no provider-specific live behavior in Effigy core
- no Render/Railway package implementation changes
- no provider resource provisioning
- no provider secret management
- no database rollback
- no release command execution from deploy
- no `.github/workflows/` edits
- no release execution

## Desired Ownership

Deploy command code should split into these owners:

- command dispatch: maps CLI args to plan/apply/status/history/redeploy actions
- transaction planning: builds provider-neutral plan/apply/status/history data
- manifest deploy config parsing: owns `[deploy.<env>]` parsing and validation
- report models: owns serializable plan/apply/status/history/redeploy shapes
- report persistence: owns active/latest/history path conventions and JSON
  read/write helpers
- provider-package adapter: owns external provider package resolution and phase
  invocation, not provider-specific behavior
- text rendering: owns human-facing summaries only

## Crate Boundary Rule

An `effigy-deploy` crate is allowed only if a stable domain boundary emerges
that is useful outside the runner. File-size reduction alone is not enough.

The default path is internal module decomposition under `src/runner/deploy_command/`.

## Compatibility Rules

- `deploy model` and `deploy export` remain static derivation/export surfaces.
- `deploy plan`, `apply`, `status`, `history`, and `redeploy` remain live
  transaction surfaces.
- provider package phase scripts keep the same context and report contracts.
- active/latest/history paths remain stable.
- text rendering may move, but output should not change except for deliberate
  whitespace cleanup scoped by a card.

## First Classification Targets

`675` must classify the current deploy code across:

- `src/runner/deploy_command/transaction.rs`
- `src/runner/deploy_command/provider_package.rs`
- `src/runner/deploy_command/model.rs`
- `src/runner/deploy_command/derive.rs`
- deploy runner and JSON contract tests

## Current Ownership Map

`675` confirmed the current deploy implementation is split as follows:

- `transaction.rs`: command entrypoints for `plan`, `apply`, `status`,
  `history`, and `redeploy`; deployment config parsing; plan construction; code
  ref resolution; provider preflight/apply/status orchestration; provider
  context assembly; provider policy checks; active/latest/history report path
  helpers; JSON read/write helpers; text rendering; deploy transaction report
  structs.
- `provider_package.rs`: provider package manifest parsing; path/git source
  materialization; descriptor validation; phase script workspace/context/report
  handling; phase report validation; scoped env overrides.
- `model.rs`: static `deploy.model.v1` domain model plus generic export result
  and warning helpers.
- `derive.rs`: derives provider-neutral deploy model from effective manifest,
  bundle defaults, and child task manifests.
- provider export scripts: static file export lives in configured provider
  packages, not core Rust modules.

## Test Coverage Map

Existing coverage for this lane:

- `src/tests/runner_tests/runner_core_tests/deploy_tests.rs` covers deploy model
  derivation, Render/Railway static export, deploy plan/apply/status/history/
  redeploy, provider package fixture scripts, required release blockers, and
  explicit manifest deploy models.
- `src/tests/json_contract_tests/deploy_contract_tests.rs` covers
  `deploy.model.v1` and `effigy.deploy.export.v1` JSON contracts.
- CLI parse tests cover deploy command grammar. This lane should not need to
  change them unless code movement exposes a parser dependency.

## First Implementation Slice

`676` should extract deploy report models and active/latest/history path helpers
first.

This is the safest split because:

- the serializable report shapes are already stable JSON contracts
- report paths are pure and independent of provider execution
- text rendering can continue to consume the same types after the move
- provider package behavior remains untouched

## Acceptance Boundary

This contract is satisfied when:

- deploy transaction models have a clear owner
- deploy report persistence has a clear owner
- provider package context/dispatch is isolated from text rendering
- JSON contract tests remain stable
- provider-specific live behavior remains outside Effigy core
- `deploy export` remains compatible and separate from live deployment

## Accepted Shape

The accepted v0.6.x cleanup shape is:

- `transaction.rs`: deploy transaction orchestration.
- `report.rs`: deploy report schemas, active/latest/history path conventions,
  JSON report persistence, and JSON conversion helpers.
- `provider_context.rs`: provider context JSON assembly and provider package
  policy blockers.
- `text.rs`: human-facing deploy transaction rendering.
- `provider_package.rs`: provider package source resolution, descriptor
  validation, Rhai phase invocation, and phase report validation.

This lane deliberately did not create an `effigy-deploy` crate. The split is
useful, but current deploy internals still depend on runner side effects enough
that a new crate would be premature.
