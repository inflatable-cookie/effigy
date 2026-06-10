# Post-Rhai Foundation Migration Boundary Decision

Date: 2026-04-14  
Roadmap: `g02.004`  
Batch card: `088-decide-post-rhai-foundation-migration-slice.md`

## Decision

The next Rhai migration slice is an Effigy-only dogfooding batch.

That means:

- more Effigy shell-glue migration comes next
- Keepsake stays as the first cross-repo pilot after stronger dogfooding
  evidence exists
- Jetstream is explicitly deferred for now because active local work there
  makes it the wrong immediate migration target

## Why

The Rhai script-step foundation is now real, but one migrated helper is not
enough evidence to start spreading the surface across repos.

Effigy should first use its own scripting surface on a meaningful cluster of
repo-local automation glue so the next host-API gaps are discovered in the
first-party repo, not in a consumer pilot.

This keeps the lane honest:

- no simultaneous multi-repo churn
- no premature Keepsake pilot
- no Jetstream interference while active work is happening there

## Resulting Ready Card

- [`089-implement-effigy-rhai-dogfooding-cluster.md`](../specs/batch-cards/089-implement-effigy-rhai-dogfooding-cluster.md)

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `ADOPT`
- Movement:
  - the scripting lane moved from “which repo next?” ambiguity to one explicit
    Effigy-only dogfooding batch
- Remaining open:
  - which Effigy shell-glue cluster should migrate first
  - what host-API gaps the dogfooding pass exposes

## Next Task

Execute `089-implement-effigy-rhai-dogfooding-cluster.md`.
