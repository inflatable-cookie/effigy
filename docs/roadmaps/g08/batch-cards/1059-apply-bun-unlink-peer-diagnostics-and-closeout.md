# 1059 - Apply Bun Unlink, Peer Diagnostics, And Closeout

Roadmap: [`../021-bun-local-dependency-linking.md`](../021-bun-local-dependency-linking.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1058`

## Purpose

Complete reversible Bun linking: remove exactly one desired consumer closure,
release only provably unshared Effigy-owned registrations, diagnose duplicate
framework peers, and close `g08.021` with real round-trip proof.

## Owner And Seam

`effigy-deps` owns unlink planning/application, index reference release,
immutable-file guards, physical verification, peer diagnosis, and rollback.
The runner only dispatches and renders shared operation reports.

## Work

- revalidate exact ledger, index, immutable-file, registration, and consumer
  symlink preconditions under the registration-index lock
- remove only exact, revalidated consumer symlinks in the selected closure;
  Bun has no consumer-side multi-package unlink command
- release selected consumer references atomically; unregister only an
  Effigy-created last reference whose observed target still matches exactly,
  using package-directory `bun unlink --no-save`
- preserve shared, foreign, stale, conflicting, or unverifiable registrations
  and report why they were retained
- rollback consumer links, registration state, ledger, and index on process,
  invariant, persistence, or verification failure without touching foreign
  state
- keep every consumer/library `package.json`, `bun.lock`, and `bun.lockb`
  byte-for-byte unchanged across success and failure
- make unlinked unlink a successful no-op with text and JSON evidence
- detect duplicate framework peer resolution, including Svelte, and report
  both resolved paths with actionable dedupe guidance
- wire `effigy deps unlink bun <path>` text, JSON, and dry-run
- prove real Bun `1.3.14` link/edit/unlink/re-link behavior for root-only and
  multi-package closures, including shared-registration retention
- close `g08.021` and ready the first bounded `g08.022` doctor/hygiene card

## Guardrails

- no process invocation without explicit `--no-save`
- no manifest or lockfile mutation tolerance
- no partial-closure unlink or foreign/shared registration removal
- no Git restore command or broad `node_modules` cleanup
- no doctor implementation or portfolio consumer mutation in this card

## Acceptance

- [x] dry-run and already-unlinked paths execute no Bun process or state write
- [x] unlink removes the complete selected consumer closure or rolls back
- [x] another desired consumer retains its links and registration references
- [x] only a matching last-reference Effigy registration is unregistered
- [x] foreign, shared, stale, and unverifiable registrations remain untouched
- [x] manifest and lockfile bytes remain identical across success and failure
- [x] post-unlink verification proves selected symlinks absent
- [x] peer duplication reports exact package paths and remediation
- [x] text and JSON expose intents, outcomes, retained state, and rollback
- [x] real root-only and multi-package link/unlink/re-link proofs pass
- [x] `g08.021` closes with a ready `g08.022` batch card

## Validation

- focused unlink/precondition/refcount/rollback fixtures
- peer duplication and healthy-dedupe fixtures
- real Bun root-only and multi-package round trips
- focused CLI text/JSON/dry-run/no-op tests
- `cargo test -p effigy-deps`
- `cargo clippy -p effigy-deps --all-targets -- -D warnings`
- `effigy qa:ci:fast`
- `effigy qa:ci:json`
- `effigy qa:docs`
- `git diff --check`

## Stop Conditions

Stop and replan if exact consumer symlinks cannot be removed without manifest
or lockfile churn, safe package-directory registration removal cannot be proven
under the index lock, peer duplication cannot identify both physical paths, or
rollback would remove foreign/shared machine state.

## Next Task

Execute ready observed-health card `1060` under `g08.022`.
