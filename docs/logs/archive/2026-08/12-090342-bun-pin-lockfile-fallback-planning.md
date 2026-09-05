# Bun Pin Lockfile Fallback Planning

Status: complete
Created: 2026-08-12
Roadmap: g08.031
Batch: pin-lockfile-fallback-planning

## Summary

- reproduced `bun pm ls --all` `InvalidPackageInfo` failures through
  `deps pin bun --dry-run` in five consumers
- proved cp-front still succeeds through process inventory
- reopened strict spec `104` with ready card `1081`
- narrowed the fix to a pin-only, warning-bearing text-lockfile fallback

## Evidence

- affected: cp-admin, compli-me/front, bloom, greenhouse, and cream
- control: cp-front reports the existing Poodle closure successfully
- each affected text `bun.lock` has a JSONC-readable `packages` object
- nested package keys require identity derivation from the record's first
  package specifier rather than the lock key
- all reproduction commands were dry-run or read-only; no consumer changed

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `MAINT`, `AGENT`
- Movement: pin enumeration was coupled to one failing Bun subcommand -> a
  bounded fallback contract and ready implementation card now exist
- Remaining gap: implement card `1081`, prove all six consumers, and close the
  open papercut

## Validation Performed

- `effigy deps pin bun ../../poodle --dry-run` with each consumer selected by
  leading `--repo`
  - result: five matching `InvalidPackageInfo` failures; cp-front success
- `bun -e` using `Bun.JSONC.parse` against cp-admin `bun.lock`
  - result: 364 package records and a readable Poodle package specifier
- `effigy qa:docs`
  - result: pass, including links, indexes, workflow paths, and vision
    next-action policy
- targeted link check for spec `104`, card `1081`, and this log
  - result: pass
- `git diff --check`
  - result: pass

## Risks

- an ad hoc JSONC stripper could misparse comments or trailing commas
- weakening shared inventory would make link safety decisions from declared
  rather than installed state
- silently falling back would hide Bun degradation from operators

## Next Task

Execute ready card `1081`.
