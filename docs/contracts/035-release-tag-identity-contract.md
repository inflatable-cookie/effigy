# 035 Release Tag Identity Contract

Status: Active
Owner: Platform
Created: 2026-08-06

## Purpose

Define the Git object Effigy creates when an operator authorizes
`release execute --yes`.

## Tag Identity

Effigy creates an annotated Git tag for every release. The annotation message
is exactly the rendered tag name.

For example, a configured `v{version}` format and version `0.1.0` produce:

- tag ref: `refs/tags/v0.1.0`
- Git object type: `tag`
- annotation message: `v0.1.0`

The message rule is deterministic and needs no second configuration field.
Release planning already exposes the rendered tag, so an operator can approve
both the ref and annotation before execution.

Effigy does not create:

- a lightweight tag pointing directly at a commit
- a signed tag without separate future signing authority and configuration
- a second tag when the local ref already exists
- a tag before release preparation and execute preflight succeed

## Execution And Push

`release execute --yes` must:

1. create the release commit
2. create one annotated tag over that commit
3. push the branch
4. push the same tag object to `origin`
5. remove prepared state only after the complete push succeeds

Existing no-retag, partial-failure, and exact prepared-state rules remain
unchanged. A tag creation or push failure remains explicit and does not permit
automatic retagging.

## Evidence

Deterministic release-execute fixtures must prove:

- the local ref resolves to an object of type `tag`
- the annotation message exactly equals the rendered tag
- dereferencing the tag resolves to the release commit
- the bare remote receives an object of type `tag` with the same message
- existing local-tag collision and partial-push failure behavior stays closed

## Change Policy

Changing tag type, annotation-message derivation, signing posture, or push
ordering requires contract, guide, release corpus, and operator-handoff review.
