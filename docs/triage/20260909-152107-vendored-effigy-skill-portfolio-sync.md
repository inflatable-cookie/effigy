# Vendored Effigy Skill Portfolio Status and Sync

Status: open — unresolved deferred candidate
Created: 2026-09-09
Owner: Agent adoption and skill distribution (reassigned 2026-09-05 docs
  cleanup; orchestrator does not own triage)
Source: retired roadmap backlog
  `docs/roadmaps/backlog/vendored-effigy-skill-portfolio-status-and-sync.md`
  (2026-08-31, from Northstar papercuts wave 23 and the open Effigy
  `PAPERCUTS.md` entry); preserved here on backlog retirement without change
  of meaning
Depends on: completed `g08.037` external skill task runner
Papercuts: [`PAPERCUTS.md`](../../PAPERCUTS.md) (open entry "Vendored Effigy
  skills need portfolio-level status and sync", 2026-08-30)

## Purpose

Give maintainers a supported way to see and repair drift in Effigy-managed
skill copies across a bounded repository portfolio.

This follows the external skill runner without becoming part of it:

- `g08.037` runs an explicit installed skill task source against one consumer.
- this candidate inventories and synchronizes Effigy skill installations across
  multiple consumers.

This note is non-authoritative. Do not create a worker or ready task from
this note.

## Scope

Start from a JSON-first scoped status/sync workflow that:

- inventories repo-local managed Effigy skill installations below an explicit
  portfolio root;
- fingerprints the bundled source version and each installed managed file;
- distinguishes current, stale, missing, unmanaged, and dirty installations;
- refuses to overwrite dirty skill trees;
- updates only Effigy-managed files and reports every skipped repository;
- remains explicit about scope and never scans arbitrary machine locations.

The planning pass must decide whether this belongs under `effigy skill`,
`effigy init`, or a shared agent-adoption owner. The completed `skill tasks` /
`skill run` surface does not settle that command ownership by itself.

## Constraints

- no automatic background synchronization;
- no overwrite of user-authored or dirty skill files;
- no global installed-skill registry;
- no widening of card `1089` or the active documentation-context lane;
- no claim that every repository under a portfolio root is an Effigy consumer.

## Open Questions

- Which command owns the portfolio surface: `effigy skill`, `effigy init`,
  or a shared agent-adoption owner?
- What inventory shape classifies every install in the reported 15-repository
  cohort without an ad hoc shell loop?

## Promotion Conditions

Primary tags: `MAINT`, `OPERATE`, `ROUTE`.

Target envelope: one explicit portfolio command reports managed Effigy skill
drift in stable text/JSON and can apply a bounded, dirty-tree-safe
synchronization plan.

Promotion signals:

- inventory against the reported 15-repository cohort classifies every install
  without an ad hoc shell loop;
- dirty, unmanaged, and missing installations have distinct non-destructive
  outcomes;
- status is read-only and sync names the exact managed-file mutation set before
  apply;
- command ownership is settled against `skill`, `init`, and agent-adoption
  responsibilities;
- an active execution window exists after the current ready documentation lane.

Promotion requires operator intent, current canonical refs, an active
generation, and a ready top-level Northstar task.

## Next Task

Keep the matching `PAPERCUTS.md` entry open until this candidate is promoted
or deliberately declined. Next check: agent-adoption evidence that portfolio
drift blocks consumer work, or the next operator-led strategic runway
checkpoint, whichever comes first. Do not create a worker or ready task from
this note.
