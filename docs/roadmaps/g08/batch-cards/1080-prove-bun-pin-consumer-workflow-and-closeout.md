# 1080 - Prove Bun Pin Consumer Workflow And Closeout

Roadmap: [`../031-bun-committed-dependency-pinning.md`](../031-bun-committed-dependency-pinning.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md),
[`../../../contracts/040-bun-committed-dependency-pinning-contract.md`](../../../contracts/040-bun-committed-dependency-pinning-contract.md)
Spec: [`../../../specs/archive/104-bun-committed-dependency-pinning.md`](../../../specs/archive/104-bun-committed-dependency-pinning.md)

Status: Complete
Owner: Consumer proof and documentation
Created: 2026-08-11
Ready after: card `1079` closed with command and JSON contracts green

## Purpose

Prove the committed override on the motivating multi-repository graph, publish
the operator boundary, and close the lane.

## Owner And Seam

This card owns disposable consumer proof, public guidance, and planning
closeout. Runtime semantics remain under contracts `034` and `040`.

## Work

- exercise dry-run, pin, operator-run `bun install`, verification, unpin, and
  re-install in disposable Soundcheck/Poodle copies
- prove the complete Poodle package closure is pinned once at the consumer
  root and duplicate Svelte package identity disappears
- prove intermediate Soundcheck-library and Longhorn repositories remain
  untouched
- prove status still reports external linked-package contamination independent
  of the consumer override
- update guide `077`, docs front doors, command matrix, agent skill copies, and
  changelog with the committed-versus-ephemeral boundary
- run focused consumer proof, full docs/JSON checks, Clippy, and full QA
- close roadmap/cards/spec/front doors, archive spec `104`, and write one dated
  evidence log

## Acceptance

- [x] disposable consumer proof matches contract `040` end to end
- [x] install is operator-owned and outside Effigy's mutation report
- [x] package identity proof is clean after pin/install and restored after
      unpin/install
- [x] no source consumer or intermediate repository is mutated by proof
- [x] public docs never imply that link commits or pin stays machine-local
- [x] full QA passes and no stale ready card remains

## Validation

- disposable Soundcheck/Poodle command and type-check proof
- byte/status checks across consumer, library, and intermediate repositories
- `effigy qa:docs`
- `effigy qa:json`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- full `effigy qa`
- `git diff --check`

## Evidence Requirement

Close with one dated log containing the exact disposable topology, commands,
package closure, type-check result, untouched-repo proof, full validation, and
archive/front-door updates.

Evidence: [`11-234531-bun-pin-consumer-proof-and-closeout.md`](../../../logs/2026-08/11-234531-bun-pin-consumer-proof-and-closeout.md)

## Stop Conditions

Stop if proof needs mutation of a real consumer checkout, automatic install,
cross-repository writes, suppression of physical contamination warnings, or
new resolver semantics beyond contract `040`.

## Next Task

Card complete. No ready card remains in this lane.
