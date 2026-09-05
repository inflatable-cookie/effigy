# g08.024 - Initial Current-Version Release Tag

Status: Complete
Depends on: `g08.023`

## Goal

Let a repository that already declares its intended first release version use
Effigy's normal release planning and simulation path without inventing a prior
version or skipping release orchestration.

## Vision Alignment

- Primary tags: `RELEASE`, `CONTRACT`, `OPERATE`
- Target envelope: first-tag release planning remains explicit, bounded, and
  subject to the same gates as every later release.
- Vision target delta: Effigy can now model the initial tag at the version
  already present in the repository.

## Scope

- add explicit `release.initial-tag-current-version` configuration
- permit equality with the current version only when the changelog contains no
  released versions
- reject lower versions and close the exception after the first release
- reject an existing matching local tag
- omit a no-op version-file mutation while retaining changelog promotion and
  configured release gates
- keep manifest schema validation and operator guidance current

## Non-Goals

- no change to default monotonic release behavior
- no automatic first-release inference
- no tag creation, release execution, or remote mutation
- no exception for repositories with released changelog history

## Acceptance Criteria

- [x] mode is disabled by default and requires an explicit boolean opt-in
- [x] status and planning select the current version only for a first release
- [x] prepare planning omits the unchanged version file
- [x] existing tags, released changelog history, and lower versions fail closed
- [x] the CLI override and direct release-library paths share one validator
- [x] schema, focused tests, clippy, docs, and a real consumer simulation pass

## Evidence

- [`Initial current-version release tag closeout`](../../logs/archive/2026-08/06-111534-initial-current-version-release-tag.md)
- [`Release orchestration guide`](../../guides/051-release-orchestration.md)

## Next Task

Select the next substantial g08 scope separately. No release execution or
generation rollover is implied.
