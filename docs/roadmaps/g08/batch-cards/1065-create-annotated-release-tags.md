# 1065 - Create Annotated Release Tags

Roadmap: [`../025-annotated-release-tag-integrity.md`](../025-annotated-release-tag-integrity.md)
Contract: [`../../../contracts/035-release-tag-identity-contract.md`](../../../contracts/035-release-tag-identity-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-06
Ready after: operator-selected g08 scope

## Purpose

Repair the Git mutation primitive so authorized release execution creates the
contracted annotated tag with a deterministic message.

## Owner And Seam

`effigy-release` owns release Git mutations. This card changes only tag
creation and focused evidence; release planning, commit creation, push order,
prepared state, and failure recovery remain unchanged.

## Work

- create the tag with Git's annotated-tag mode
- use the exact rendered tag as the annotation message
- keep argument boundaries safe for Git option parsing
- add focused deterministic proof for object type, message, and dereferenced
  commit identity
- leave signing and configurable tag messages out of scope

## Acceptance

- [x] a created release tag has Git object type `tag`
- [x] its annotation message exactly equals the rendered tag
- [x] dereferencing it resolves to the intended commit
- [x] existing tag-collision and failure behavior remains unchanged
- [x] focused `effigy-release` validation passes

## Validation

- focused `effigy-release` tests
- formatting and focused Clippy
- `git diff --check`

## Stop Conditions

Stop and replan if annotated creation requires changing the release config or
JSON schema, weakens no-retag behavior, or introduces signing authority.

## Next Task

Card 1066 completed execute proof and lane closeout. Select the next substantial
g08 scope separately.
