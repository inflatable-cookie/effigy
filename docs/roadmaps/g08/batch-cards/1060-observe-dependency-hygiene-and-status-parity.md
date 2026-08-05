# 1060 - Observe Dependency Hygiene And Status Parity

Roadmap: [`../022-dependency-link-doctor-and-hygiene.md`](../022-dependency-link-doctor-and-hygiene.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1059`

## Purpose

Make dependency-link health one manager-neutral, read-only observation model and
expose the same evidence through `effigy deps status` text and JSON.

## Owner And Seam

`effigy-deps` owns inspection, classification, evidence, and remediation data.
The runner only renders the shared status report. Doctor integration remains in
`1061`.

## Work

- extend observed state for Cargo managed-config drift, tracked/conflicting
  config, and linked-library path-source lock entries
- distinguish healthy Bun closures, complete link loss, partial local/registry
  closure, registration conflicts, and immutable manifest/lock drift
- include Bun peer-resolution diagnostics in read-only status inspection
- report manager, mechanism, library, packages, exact evidence, severity, and
  remediation through shared typed findings
- keep text and JSON deterministic and additive under the standard command
  envelope
- prove status inspection performs no writes or manager processes

## Guardrails

- no doctor adapter or doctor rendering in this card
- no package-manager mutation, repair, or automatic cleanup
- no manifest, lockfile, desired-state, or ownership-index writes
- no duplicate health classification in the runner

## Acceptance

- [x] Cargo lock path-source state is a named do-not-commit error
- [x] Cargo config drift and tracked/conflicting config have exact evidence
- [x] Bun full loss, partial closure, registration conflict, and peer duplicate
      are distinct observations
- [x] every unhealthy observation carries actionable remediation
- [x] text and JSON expose the same manager/library/package evidence
- [x] healthy and unhealthy inspection fixtures prove zero mutation

## Validation

- focused shared-observation fixtures for Cargo and Bun
- focused status text/JSON tests
- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `effigy qa:ci:fast`
- `git diff --check`

## Stop Conditions

Stop and replan if health classification requires mutating either manager,
status cannot consume one shared observation shape, or exact do-not-commit
evidence cannot be recovered without interpreting runner output.

## Next Task

Execute ready doctor-integration card `1061`.
