# g05.020 - Reusable Core Hardening Suite

Status: Complete
Depends on: `g05.019`

## Goal

Turn the reusable-codebase sweep findings from 2026-05-14 into a bounded
hardening tranche for the v0.7.0 release posture.

The theme is simple: core Effigy should stay provider-neutral,
product-neutral, predictable, and easy to extend from external provider and
bundle repos.

## Evidence

- the 2026-05-14 reusable-codebase sweep found no immediate release blocker
- provider-specific Render and Railway Rust code has been removed, but the
  provider package runtime still needs stronger contract validation
- active contracts still name Underlay and Decodelabs as active anchors
- source materialization logic is duplicated between bundle sources and deploy
  provider packages
- `state_command.rs`, `effigy-state`, `effigy-rhai` tests, CLI help topics, and
  test fixtures remain visible cleanup hotspots

## Ordered Follow-Up Lanes

1. [`021-deploy-provider-contract-hardening.md`](./021-deploy-provider-contract-hardening.md)
2. [`022-provider-source-materialization-convergence.md`](./022-provider-source-materialization-convergence.md)
3. [`023-active-docs-product-neutrality-cleanup.md`](./023-active-docs-product-neutrality-cleanup.md)
4. [`024-state-domain-extraction-follow-through.md`](./024-state-domain-extraction-follow-through.md)
5. [`025-low-risk-deduplication-follow-through.md`](./025-low-risk-deduplication-follow-through.md)
6. [`026-rhai-host-surface-and-test-ownership.md`](./026-rhai-host-surface-and-test-ownership.md)
7. [`027-process-execution-boundary-review.md`](./027-process-execution-boundary-review.md)

## Execution Guardrails

- do not reintroduce Render, Railway, Underlay, Decodelabs, or Example App as
  core Rust concepts
- do not edit files under `external/` unless the user explicitly asks for
  submodule/provider-repo work
- do not remove historical references from changelogs, logs, archived specs, or
  old roadmap history
- keep provider package behavior portable through plain JSON, TOML, YAML, Rhai,
  and documented file/env surfaces
- prefer small domain moves with compatibility tests over sweeping rewrites
- run focused tests after each batch, then one wider `cargo test`/QA pass at the
  end of a substantial tranche

## Non-Goals

- no release execution
- no new provider implementation in core
- no provider account provisioning
- no redesign of the deploy model schema unless a later contract explicitly
  opens that work
- no generation rollover while this tranche remains active

## Acceptance Criteria

- provider package context and report contracts are stricter and covered by
  golden tests
- active docs/contracts no longer present product-specific bundles as core
  anchors
- duplicated source-materialization logic is either converged or explicitly
  deferred with evidence
- high-value duplicate blocks are reduced or documented as intentionally
  retained
- the Rhai provider-facing surface is accurately documented and tested
- the final closeout records validation and any accepted residual risk

## Outcome

Completed through `742` to `749`.

Landed:

- typed deploy-provider context and stricter provider report validation
- shared git source cache identity and shared git subprocess output/error
  projection where duplication was real
- product-neutral active docs posture for external provider packages
- further state-domain extraction into `effigy-state`
- duplicate reduction for bootstrap/release fixture setup
- Rhai host-surface test ownership split
- process-boundary classification without a broad facade rewrite

Accepted residual risk:

- `src/runner/state_command.rs` and `crates/effigy-release/src/lib.rs` remain
  warning-level god files
- duplicate-block scan still shows `94` findings with `6` high findings, mainly
  in CLI help topics and one container temp helper pair
- provider-package OCI source materialization remains explicitly unsupported

## Suggested Batch Cards

- open reusable-core hardening lane
- harden deploy provider context/report contracts
- converge provider and bundle source materialization
- neutralize active product-specific docs/contracts
- finish state-domain thin-shell follow-through
- reduce high duplicate blocks and add local fixture builders
- split Rhai host-surface tests and refresh docs
- review process execution boundary and document retained direct calls
- close reusable-core hardening proof

## Next Task

Reusable-core hardening is closed. Open a new lane only if a fresh v0.7.0
hardening tranche is explicitly planned.
