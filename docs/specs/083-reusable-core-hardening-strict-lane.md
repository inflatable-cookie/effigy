# 083 - Reusable Core Hardening Strict Lane

Roadmap: [`g05.020`](../roadmaps/g05/020-reusable-core-hardening-suite.md)
Contracts:
- [`025-deploy-provider-package-contract.md`](../contracts/025-deploy-provider-package-contract.md)
- [`027-state-domain-extraction-contract.md`](../contracts/027-state-domain-extraction-contract.md)
- [`029-deploy-domain-boundary-contract.md`](../contracts/029-deploy-domain-boundary-contract.md)
- [`030-low-risk-deduplication-contract.md`](../contracts/030-low-risk-deduplication-contract.md)
- [`031-artifact-and-crate-boundary-contract.md`](../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-14
Completed: 2026-05-14

## Purpose

Execute the reusable-core hardening suite for v0.7.0 without reopening broad
product planning. This lane tightens provider contracts, keeps core
provider-neutral, and closes the most visible maintainability debt found in the
2026-05-14 sweep.

## Lane Posture

Posture: `strict-active`

This lane is executable because the roadmap tranche is written, the audit
findings are concrete, and the next slices can be landed through bounded owner
seams.

## Hard Boundaries

- no release execution
- no `.github/workflows/` edits
- no provider-specific Rust behavior in core
- no edits under `external/` unless the user explicitly asks for submodule or
  provider-repo work
- no historical-reference scrubbing in changelogs, logs, archived specs, or old
  roadmap files
- no deploy-model schema redesign unless a later card explicitly opens it
- no speculative crate merges or wide process-execution rewrite

## Execution Order

1. `741` complete: lane opened and ready chain wired
2. `742` complete: deploy-provider contract hardening
3. `743` complete: provider source materialization convergence
4. `744` complete: active docs product-neutrality cleanup
5. `745` complete: state-domain extraction follow-through
6. `746` complete: low-risk deduplication follow-through
7. `747` complete: Rhai host-surface and test ownership follow-through
8. `748` complete: process execution boundary review
9. `749` complete: reusable-core hardening closeout

## Ready Chain

- `741` is complete
- `742` is complete
- `743` is complete
- `744` is complete
- `745` is complete
- `746` is complete
- `747` is complete
- `748` is complete
- `749` is complete

## Auto-Continuation Envelope

Auto-start is enabled for this lane while:

- the previous card closes green
- no new contract gap appears during implementation
- no provider package behavior needs product-specific Rust logic in core
- no external provider repo edit is required to prove the current slice
- no fresh schema redesign judgment becomes necessary

Stop and replan if implementation discovers:

- the provider contract needs a wire-shape break
- OCI provider-package materialization needs broader delivery planning than the
  current card allows
- active docs neutrality requires contract promotion rather than bounded docs
  repair
- process boundary work wants a broad facade instead of a small shared helper

## Acceptance

This lane is complete when:

- deploy-provider context/report contracts are hardened and proved
- active docs no longer present product-specific bundles as core anchors
- source-materialization duplication is reduced or explicitly deferred with
  evidence
- state, Rhai, and duplicate-block follow-through slices either land or are
  deliberately deferred with evidence
- currentness surfaces point at the next queued tranche or no active lane

## Outcome

This lane is closed green.

Residual risk retained at closeout:

- `src/runner/state_command.rs` remains a warning-level god file
- `crates/effigy-release/src/lib.rs` remains a warning-level god file
- duplicate-block scan remains at `94` findings with `6` high findings
- provider-package OCI materialization remains intentionally unsupported
- no further shared subprocess abstraction was justified beyond the git helper

## Next Task

None. Lane `083` is closed.
