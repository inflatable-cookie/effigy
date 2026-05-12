# 677 - Isolate Provider Package Dispatch Context

Roadmap: [`../037-deploy-domain-boundary-hardening.md`](../037-deploy-domain-boundary-hardening.md)
Strict lane: [`../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md`](../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md)
Contract: [`../../../contracts/029-deploy-domain-boundary-contract.md`](../../../contracts/029-deploy-domain-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Separate deploy provider package dispatch context assembly from transaction
planning and text rendering.

## Scope

- extract provider context construction into a narrow module or function owner
- extract provider package policy blocker mapping if it belongs with dispatch
- keep provider package phase execution behavior unchanged
- keep provider context JSON schema unchanged
- keep provider package Rhai environment behavior unchanged

## Non-Goals

- no provider package descriptor grammar changes
- no provider package script behavior changes
- no provider source materialization changes
- no Render/Railway package changes
- no text rendering split yet

## Acceptance

- `transaction.rs` no longer constructs provider package context inline
- provider phase scripts receive the same context shape
- provider policy blockers are owned by a clear provider-dispatch boundary
- deploy provider fixture tests still pass

## Outcome

- added `src/runner/deploy_command/provider_context.rs`
- moved provider context JSON assembly into a narrow request builder
- moved provider package policy blocker mapping into the provider context owner
- kept provider package phase execution unchanged
- reduced `transaction.rs` from 951 lines to 924 lines

## Validation

```sh
cargo test deploy_tests::
cargo check --bin effigy
git diff --check
```

## Next Task

Execute `678` to split deploy text rendering from transaction state.
