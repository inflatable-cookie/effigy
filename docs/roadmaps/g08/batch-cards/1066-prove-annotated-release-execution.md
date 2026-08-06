# 1066 - Prove Annotated Release Execution

Roadmap: [`../025-annotated-release-tag-integrity.md`](../025-annotated-release-tag-integrity.md)
Contract: [`../../../contracts/035-release-tag-identity-contract.md`](../../../contracts/035-release-tag-identity-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-06
Ready after: card 1065

## Purpose

Freeze the actual execute path locally and through its bare-remote fixture,
publish operator truth, and prove the repaired installed binary against
Swallowtail's read-only release handoff.

## Owner And Seam

This card owns release-execute corpus, guide/changelog truth, installed binary
proof, consumer simulation, and lane closeout. It does not authorize a real
release mutation.

## Work

- extend the existing execute-success fixture to inspect local and remote tag
  object type, annotation message, and peeled release commit
- update release guidance and changelog
- run focused and affected Effigy validation
- install the repaired local binary
- rerun Swallowtail's release simulation and read-only tag/remote checks
- close contract, roadmap, card, log, and front-door state honestly

## Acceptance

- [x] local and bare-remote refs are annotated tag objects
- [x] both annotation messages equal the rendered tag
- [x] both peeled refs resolve to the created release commit
- [x] focused and affected Effigy validation passes
- [x] Swallowtail's exact candidate remains ready under the installed binary
- [x] no real release prepare, execute, tag, or push runs

## Validation

- focused execute-success corpus
- `cargo test -p effigy-release`
- affected test selection from the graph
- formatting and focused Clippy
- `effigy qa:docs`
- Swallowtail `effigy release simulate`
- `git diff --check`

## Stop Conditions

Stop if remote proof needs network access, a real release mutation, or changes
to Swallowtail's accepted candidate.

## Next Task

Select the next substantial g08 scope separately. Swallowtail card 130 returns
to its explicit release-authorization boundary.
