# Vendored Effigy Skill Portfolio Status and Sync

Status: Queued; exploratory, not ready for execution
Owner: Agent adoption and skill distribution
Created: 2026-08-31
Source: Northstar papercuts wave 23 and the open Effigy `PAPERCUTS.md` entry
Depends on: completed `g08.037` external skill task runner

## Purpose

Give maintainers a supported way to see and repair drift in Effigy-managed
skill copies across a bounded repository portfolio.

This follows the external skill runner without becoming part of it:

- `g08.037` runs an explicit installed skill task source against one consumer.
- this item inventories and synchronizes Effigy skill installations across
  multiple consumers.

## Candidate Surface

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

## Boundaries

- no automatic background synchronization;
- no overwrite of user-authored or dirty skill files;
- no global installed-skill registry;
- no widening of card `1089` or the active documentation-context lane;
- no claim that every repository under a portfolio root is an Effigy consumer.

## Promotion Criteria

Primary tags:

- `MAINT`
- `OPERATE`
- `ROUTE`

Target envelope:
- one explicit portfolio command reports managed Effigy skill drift in stable
  text/JSON and can apply a bounded, dirty-tree-safe synchronization plan.

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

## Queue State

Keep the matching `PAPERCUTS.md` entry open until this backlog item is promoted
or deliberately declined. Do not create a worker or ready card from this file.

