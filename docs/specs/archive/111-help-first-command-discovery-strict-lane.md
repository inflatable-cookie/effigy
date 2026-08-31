# 111 Help-First Command Discovery Strict Lane

Status: Complete
Created: 2026-08-31
Closed: 2026-08-31
Roadmap: [`g08.038`](../../roadmaps/g08/038-help-first-command-discovery.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)
Completed card: [`1093`](../../roadmaps/g08/batch-cards/1093-add-help-first-command-discovery.md)
Evidence: [`2026-08-31 closeout`](../../logs/2026-08/31-233000-help-first-command-discovery-1093.md)

## Outcome

Operators can navigate Effigy's broad public surface by job through help while
existing commands and repository selectors keep their current grammar and
precedence.

## Problem

General help is a flat inventory of roughly thirty command families. Adding
executable group prefixes would make commands longer and reserve new top-level
names that repositories may already use as task selectors. The first migration
must improve discovery without widening execution grammar.

## Decisions

- General help and `effigy help` use six topics: `work`, `local`, `repo`,
  `deliver`, `extend`, and `admin`.
- Primary ownership is exactly the taxonomy in contract `043`.
- `effigy help <group>` renders one group.
- `effigy help <command>` reuses the existing detailed-help owner.
- Unknown topics fail instead of falling back to general help.
- No `effigy <group> <command>` route ships in this lane.
- Existing direct built-ins and manifest-task deferral stay unchanged.

## Scope

- command descriptor or equivalent typed help-group metadata
- grouped general-help rendering
- help group and command-topic parsing
- unknown-topic diagnostics and exit behavior
- deferred-built-in filtering across general, group, and direct help
- collision fixtures for manifest selectors named after help groups
- focused parser/help/output tests and generated reference coverage
- public docs, agent guidance where affected, changelog, evidence, closeout

## Acceptance

- every general-help entry has exactly one primary group
- the six group inventories match contract `043`
- `help <command>` and `<command> --help` expose the same facts
- unknown topics identify the valid discovery paths
- a manifest task named `repo` still runs through `effigy repo`
- `effigy help repo` works in that repository without stealing the selector
- deferred built-ins do not leak into grouped help
- no executable grouped alias or new top-level built-in is introduced
- focused validation, `effigy qa`, fmt, clippy, and diff checks pass

## Non-Goals

- command renames or removals
- grouped execution aliases
- completion grammar for nonexistent execution routes
- release/distribution separation
- catalog-pack acquisition
- S3 or Rhai provider movement

## Stop Conditions

Stop and return to the orchestrator if implementation:

- needs executable aliases or new top-level group names;
- changes direct execution, selector precedence, or output contracts;
- needs alias deprecation, hiding, warning, or removal;
- cannot preserve deferred-built-in filtering;
- exposes an ambiguous primary group not settled by contract `043`;
- requires release, provider, catalog, or consumer migration work.

## Next Task

This lane is complete and archived. Return to planning for the catalog-pack
acquisition prototype; keep S3 extraction deferred until its consumer gate is
proved.
