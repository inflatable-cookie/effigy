# 1078 - Build Bun Pin Planner And Manifest Transaction

Roadmap: [`../031-bun-committed-dependency-pinning.md`](../031-bun-committed-dependency-pinning.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/040-bun-committed-dependency-pinning-contract.md`](../../../contracts/040-bun-committed-dependency-pinning-contract.md)
Spec: [`../../../specs/archive/104-bun-committed-dependency-pinning.md`](../../../specs/archive/104-bun-committed-dependency-pinning.md)

Status: Complete
Owner: Dependency domain
Created: 2026-08-11
Ready after: contract `040` promotion and operator continuation

## Purpose

Build the domain-owned plan and manifest transaction for committed Bun pins
without exposing a public command yet.

## Owner And Seam

`effigy-deps` owns this card. It may reuse Bun package inventory and read-only
process abstractions, but the planner, manifest edit, atomic apply, and typed
report stay below CLI and runner code.

## Work

- add typed pin/unpin plan, package action, outcome, warning, write, and
  verification models
- inventory named root/workspace packages and select the complete consumer
  graph closure through read-only `bun pm ls --all`
- collapse duplicate resolved copies by package name and compute relative
  `file:` values from the consumer manifest directory
- reject absolute output, conflicting entries, invalid manifests, non-object
  `overrides`, and partial plans
- plan unpin from the library inventory alone and remove only canonical
  package/path matches
- preserve unrelated keys, order, indentation, and final-newline posture while
  adding or removing only planned entries
- compare exact planned manifest bytes before apply and use atomic replacement
- snapshot and verify `bun.lock` and `bun.lockb` remain unchanged
- prove dry-run, apply, no-op, conflict, concurrent-change, write-failure, and
  exact-unpin behavior with focused fixtures

## Acceptance

- [x] planner returns the full matched closure in deterministic package order
- [x] no-match, exact re-pin, and already-unpinned plans perform no write
- [x] one conflicting selected entry blocks every addition
- [x] generated values are relative and canonically target local package roots
- [x] escaping paths carry the required portability warning
- [x] unpin preserves same-name entries pointing elsewhere
- [x] manifest formatting and unrelated dirty edits survive round trips
- [x] concurrent changes and apply failures preserve the original bytes
- [x] neither Bun lockfile form nor any other repository changes
- [x] no public grammar, help, or JSON schema is added in this card

## Validation

- focused `cargo test -p effigy-deps` pin planner and transaction tests
- exact fixture assertions for formatting, conflicts, CAS refusal, and locks
- `cargo fmt --all -- --check`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- changed-file affected analysis before closeout
- `git diff --check`

## Evidence Requirement

Close with one dated log containing the focused test counts, manifest fixtures,
lockfile byte proof, affected analysis, and exact next-card transition.

Evidence:
[`11-224711-bun-pin-domain-foundation.md`](../../../logs/archive/2026-08/11-224711-bun-pin-domain-foundation.md)

## Stop Conditions

Stop if safe editing requires normalizing the complete manifest, unpin requires
a hidden ledger, inventory cannot cover the full matched closure, an install is
needed to plan/apply, or any write outside the root consumer manifest appears.

## Next Task

Execute ready card
[`1079`](./1079-wire-bun-pin-cli-json-and-link-interlocks.md).
