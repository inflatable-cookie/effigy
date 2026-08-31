# 043 Feature Placement And Surface Migration Contract

Status: active
Owner: product architecture, CLI routing, and extension boundaries
Created: 2026-08-31
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)

## Purpose

Keep Effigy's broad façade coherent without making core the permanent owner of
every provider implementation, concrete asset, or consumer workflow.

## Placement Contract

A capability may enter or remain in core only when Effigy owns at least one
durable deterministic invariant:

- selector or target routing;
- request, plan, or report semantics;
- lifecycle and interruption handling;
- safety, validation, redaction, or fail-closed behavior;
- transaction ordering, identity, or evidence.

Public reachability through `effigy` is not sufficient. Provider specificity,
mandatory dependency policy, security response, release coupling, consumer
evidence, and extraction seams must be recorded before placement changes.

Binary size alone cannot justify extraction or retention.

## Command Grouping Contract

The approved model is group-first and aliases-stable.

- General help groups commands by operator job before adding new grammar.
- A grouped route may be added only through the existing typed command and
  runner owners. It must not create a parallel implementation.
- Grouped and direct routes must agree on root selection, arguments, side
  effects, text facts, JSON payload/schema, diagnostics, and exit status.
- Existing direct commands remain valid when the grouped route first ships.
- Documentation, completions, agent guidance, generated references, and help
  must expose both forms and identify the preferred discovery path.
- Adding a grouped route does not mark a shortcut deprecated.
- Warning, hidden-help, or removal behavior requires a separate approved
  migration card with consumer inventory and exact compatibility evidence.
- High-frequency direct commands may remain permanent shortcuts.

Exact namespace names and command grammar are not set by this contract. A
decision prototype must prove discoverability and collision behavior first.

## Repository-Intelligence Contract

Graph, scan, docs, contracts, and papercuts remain provider-neutral core
capabilities.

A grouped discovery route:

- reuses their current implementations and output contracts;
- preserves direct `graph`, `scan`, `docs`, `contracts`, and `papercuts`
  commands initially;
- explains which job each child surface performs;
- preserves standard leading `--repo` and `--json` behavior;
- does not add a second index, policy store, or refresh lifecycle.

## Catalog-Pack Contract

Effigy core keeps catalog schema, composition, deterministic layering,
parameter resolution, selection, extraction, override ownership, and assembly.
Concrete service/workspace definitions may move to a separately versioned
default pack only when all of these hold:

- existing `service`, container, system, and task workflows need no additional
  mandatory command;
- the default pack is made available automatically during supported install or
  initialization paths;
- normal use does not depend on a surprise network fetch;
- offline/bootstrap behavior is no worse than the embedded catalog;
- project and user overrides retain precedence and provenance;
- `doctor` reports a missing, incompatible, or unhealthy pack with one direct
  repair step;
- pack version and source are visible in text and JSON diagnostics;
- migration tests replay representative current catalog workflows unchanged.

The pack transport and update policy require a decision prototype before an
implementation card can be ready.

## Release And Distribution Contract

Core retains:

- release readiness and candidate identity;
- exact-SHA gates;
- mutation ordering and human confirmation;
- irreversible-action safety;
- generic evidence and transaction reports.

Effigy-specific repository URLs, Homebrew formulae, documentation/file lists,
self-hosting checks, and publishing recipes move to repository-owned tasks or
an installed extension. A reusable distribution library may remain only for
provider-neutral models with independent consumer evidence.

The core release façade may invoke external recipes, but it must not silently
restore Effigy-specific defaults as generic behavior. Migration must prove the
Effigy repository's current release gates before removing any existing path.

## S3 Consumer Gate

The current Rhai S3 surface remains supported while `bovine-accelerator`
depends on it for media upload.

S3 extraction, deprecation, vendored-dependency removal, or base-Rhai removal
cannot become implementation-ready until evidence proves:

1. `bovine-accelerator-desktop` owns and can execute the replacement upload;
2. `bovine-accelerator` routes the relevant media path through that replacement;
3. representative upload behavior passes in the consumer environment;
4. no supported consumer path still calls the Rhai storage functions proposed
   for removal;
5. migration and rollback instructions exist for the consumer boundary.

After that gate, planning must choose the optional-provider transport and the
minimum retained object-store/Rhai contract before implementation. No cleanup
may assume those choices.

## Migration Evidence

Every feature-placement migration must record:

- old owner, new owner, and retained Effigy invariant;
- current consumers and compatibility boundary;
- dependency and release-policy movement;
- direct and grouped command parity where applicable;
- failure and repair behavior when an optional asset/provider is absent;
- docs, completion, agent-skill, JSON, and generated-reference impact;
- rollback or staged-removal path;
- focused and full Effigy validation.

## Stop Conditions

Stop and return to planning when:

- a façade route is used as the only proof of core ownership;
- a grouped command needs a second implementation;
- an alias removal lacks explicit operator approval;
- catalog externalization adds mandatory operator ceremony or weakens offline
  behavior;
- release extraction weakens exact-SHA or irreversible-action safety;
- S3 removal precedes the consumer replacement gate;
- an extension transport or namespace spelling must be invented to proceed.

## Non-Goals

- immediate command removals;
- immediate S3 extraction;
- exact namespace spellings;
- a general plugin marketplace;
- binary-size optimization;
- release execution.

## Next Task

After card `1090` closes, compile separate planning/prototype lanes for command
grouping, release/distribution separation, catalog-pack acquisition, and
repository-intelligence discovery. Keep S3 deferred until its consumer gate is
proved.
