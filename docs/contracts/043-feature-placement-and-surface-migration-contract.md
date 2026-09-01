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

The approved first migration is help-first and execution-stable.

- `effigy --help` and `effigy help` group the general inventory by operator job.
- `effigy help <group>` accepts exactly `work`, `local`, `repo`, `deliver`,
  `extend`, or `admin` and renders that primary inventory.
- `effigy help <command>` renders the same help facts as
  `effigy <command> --help` through the existing typed help owner.
- An unknown help topic fails deterministically and points at valid groups and
  commands. It must not silently fall back to general help.
- Deferred built-ins stay omitted from general, group, and direct help wherever
  current repository routing gives the manifest selector precedence.
- Every general-help entry has exactly one primary group. Cross-links in detail
  help do not create a second primary entry or execution route.
- The lane adds no `effigy <group> <command>` grammar, no new top-level built-in
  names, and no execution-routing changes. A manifest task named `repo`,
  `local`, `deliver`, `extend`, `admin`, or `work` keeps current precedence.
- Existing direct commands, arguments, side effects, text/JSON contracts,
  diagnostics, and exits remain unchanged.
- Documentation, completions where applicable, agent guidance, generated
  references, and shipped help must describe the help-first discovery path
  without advertising executable grouped aliases.
- Warning, hidden-help, deprecation, alias, or removal behavior requires a
  separate approved migration card with consumer inventory and compatibility
  evidence.

Primary ownership is fixed as follows:

| Topic | Primary commands and shapes |
| --- | --- |
| `work` | `<task>`, `<catalog>/<task>`, `tasks`, `test`, `watch`, `doctor`, `init` |
| `local` | `container`, `system`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `graph`, `scan`, `docs`, `contracts`, `papercuts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap`, `demo` |
| `extend` | `skill`, `rhai` |
| `admin` | `config`, `deps`, `secrets`, `defer`, `uninstall`, `version`, completion, help |

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
implementation card can be ready. That prototype is now fixed as follows:

- every supported installation permanently carries a compiled baseline pack;
- selection order is project override, user override, active installed default
  pack, then compiled baseline;
- independently installed packs live in a versioned user-state store and use a
  manifest with pack identity, pack version, schema version, and Effigy
  compatibility;
- explicit OCI installation requires an `oci://` reference and records the
  immutable resolved digest; explicit local-path installation remains available
  for development and recovery;
- acquisition reuses the existing artifact adapter and must not add a bespoke
  HTTP client or implicit network probe;
- installation validates before atomic activation, and a failed candidate
  leaves the prior active pack unchanged;
- an active pack that later becomes unreadable or incompatible falls back to
  the compiled baseline with a visible warning and structured selection reason;
- `doctor` reports unhealthy installed state with one direct rollback or reset
  repair;
- an official fixed repository and compatible stable channel are baseline-owned
  and cannot be redirected by installed pack content;
- no-argument channel update is tested at the planner/adapter seam but is not a
  public command until the official OCI artifact is published;
- the acquisition prototype moves no concrete catalog assets and changes no
  release workflow.

The approved prototype surface is nested under the existing `service` owner:

```text
effigy service pack status
effigy service pack install oci://...@sha256:...
effigy service pack install --path <DIR>
effigy service pack rollback
effigy service pack reset
```

The later publication lane adds `effigy service pack update` only when the
official channel exists and the command can succeed from its first release.

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
- help grouping changes execution routing or reserves a new top-level name;
- an alias removal lacks explicit operator approval;
- catalog externalization adds mandatory operator ceremony or weakens offline
  behavior;
- release extraction weakens exact-SHA or irreversible-action safety;
- S3 removal precedes the consumer replacement gate;
- an extension transport or namespace spelling must be invented to proceed.

## Non-Goals

- immediate command removals;
- immediate S3 extraction;
- executable group namespaces;
- a general plugin marketplace;
- binary-size optimization;
- release execution.

## Next Task

The acquisition prototype landed under completed card
[`1095`](../roadmaps/g08/batch-cards/1095-prototype-catalog-pack-acquisition.md)
and archived strict spec `113`. Return to planning for official pack publication
and concrete-asset cutover; that lane is not ready and needs a real OCI
coordinate plus explicit workflow-edit authority. Keep release/install wiring,
public no-argument update, and S3 out of it.
