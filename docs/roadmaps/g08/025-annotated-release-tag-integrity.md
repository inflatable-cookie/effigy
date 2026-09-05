# g08.025 - Annotated Release Tag Integrity

Status: Complete
Depends on: `g08.024`
Contract: [`035`](../../contracts/035-release-tag-identity-contract.md)

## Goal

Make Effigy's irreversible release path create and push the annotated Git tag
its consumers and operator runbooks approve.

## Vision Alignment

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`
- Target envelope: exact release identity includes tag object type and message,
  not only a ref name.
- Vision target delta: release execution stops flattening an approved annotated
  tag into a lightweight ref.

## Execution Plan

- [x] card 1065: promote the tag identity contract and repair local annotated
      tag creation
- [x] card 1066: freeze local/remote execute evidence, publish operator truth,
      prove the Swallowtail consumer handoff, and close the lane

## Goals

- [x] every Effigy-created release tag is an annotated Git object
- [x] the annotation message exactly equals the rendered tag
- [x] branch and tag push preserve the same release commit and tag object
- [x] existing execution safety and no-retag behavior remain unchanged

## Non-Goals

- no signed-tag support
- no configurable annotation templates
- no release prepare, release execute, real tag, or remote mutation
- no workflow edits

## Acceptance Criteria

- [x] focused release tests prove annotated local creation
- [x] end-to-end execute fixtures prove annotated local and bare-remote tags
- [x] release guidance names the object type and deterministic message
- [x] an installed repaired binary passes Swallowtail's read-only release
      simulation and handoff checks

## Evidence

- [`Annotated release tag integrity closeout`](../../logs/archive/2026-08/06-120729-annotated-release-tag-integrity.md)
- [`Release orchestration guide`](../../guides/051-release-orchestration.md)

## Next Task

Select the next substantial g08 scope separately. No release or generation
rollover is implied.
