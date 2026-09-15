# Catalog-Scoped Code Graph Planning

Status: complete
Created: 2026-09-15
Roadmap: g10.006
Batch: catalog-scoped-code-graph-planning

## Summary

Promoted the operator-confirmed catalog-derived monorepo graph design into
canonical architecture, contract, one ready task, and a Queue handoff.

## Changes

- Made effective catalogs the only graph partition topology.
- Defined `[catalog.graph] segmented` and `independent` posture.
- Fixed root pruning, single-catalog selection, explicit fan-out, shared versus
  independent storage, docs-context isolation, and compatibility rules.
- Published `g10.006` as the sole ready frontier with a negative no-sibling-work
  oracle.
- Removed the fully promoted triage note.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`
- Movement: whole-repository graph cost had no monorepo boundary -> catalog-
  scoped lazy indexing and opt-in physical isolation have executable authority.
- Remaining gap: implementation, independent review, merge, lifecycle closeout,
  and later Acowtancy consumer adoption evidence.

## Validation Performed

- `effigy graph explore` and `effigy docs context` were used to confirm current
  code and documentation owners.
- `git diff --check` and `effigy qa:docs` validate the planning batch before
  publication.

## Risks

- Shared-store scope migration must not delete or leak sibling records.
- Documentation context currently shares graph machinery and must retain its
  separate repository-owned corpus.
- V1 deliberately rejects segmented catalogs outside the workspace root.

## Next Task

Dispatch `g10.006` through Northstar Queue.
