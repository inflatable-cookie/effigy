# Feature Placement And Command Surface Architecture

Status: active
Updated: 2026-08-31
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Effigy has grown from a task runner into a broad operator and agent surface.
The breadth is useful, but command reachability, implementation ownership, and
dependency ownership have become easy to conflate.

This architecture separates those questions. Effigy remains one obvious entry
point while provider code, concrete assets, and consumer workflows move to the
owner that can evolve them safely.

## Core Test

A capability belongs in Effigy core when Effigy owns deterministic routing,
planning, lifecycle, safety, or transaction semantics that consumers should not
recreate.

Universality is useful evidence, not the deciding test. A capability may be
core without applying to every repository. Conversely, an implementation does
not become core merely because `effigy` can invoke it.

Use these placement classes:

| Class | Effigy owns | External owner holds |
| --- | --- | --- |
| Core | routing, plans, lifecycle, safety, transactions, stable reports | task bodies and consumer policy |
| Reusable domain seam | provider-neutral models and libraries | product-specific orchestration |
| Optional provider/runtime | adapter contract and façade routing when needed | provider dependencies and release policy |
| Installed asset/extension | selection, provenance, validation, isolation | versioned recipes, catalogs, or task code |
| Consumer workflow | safe invocation and evidence where useful | product behavior and operational policy |
| Removal lane | migration diagnostics and compatibility evidence | obsolete implementation after consumers move |

Binary size is not a placement criterion. Dependency ownership, security
response, release coupling, command coherence, and real consumer evidence are.

## Operator Surface

The approved first migration is help-first and execution-stable.

1. `effigy --help` and `effigy help` present commands by operator job instead
   of one flat list.
2. `effigy help <group>` presents one grouped inventory.
3. `effigy help <command>` presents the existing detailed command help and is
   equivalent in facts to `effigy <command> --help`.
4. The first lane adds no `effigy <group> <command>` execution aliases and
   reserves no new top-level built-in names.
5. Existing direct commands and manifest-selector routing remain unchanged.
6. Adding help groups does not approve deprecation. Any later executable alias,
   warning, hiding, or removal needs a separate migration decision and evidence.

The exact primary help taxonomy is:

| Topic | Primary commands and shapes |
| --- | --- |
| `work` | `<task>`, `<catalog>/<task>`, `tasks`, `test`, `watch`, `doctor`, `init` |
| `local` | `container`, `system`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `graph`, `scan`, `docs`, `contracts`, `papercuts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap`, `demo` |
| `extend` | `skill`, `rhai` |
| `admin` | `config`, `deps`, `secrets`, `defer`, `uninstall`, `version`, completion, help |

Each general-help entry has one primary home. Detailed help may cross-link
borderline capabilities such as `bootstrap`, `demo`, or `secrets` without
duplicating execution routes.

## Repository Intelligence

Graph, scan, docs, contracts, and papercuts remain Effigy core. They provide
provider-neutral, deterministic repository navigation and policy evidence for
operators, agents, and CI.

Grouping improves discovery; it does not justify a second implementation or an
optional binary. `effigy help repo` discovers the family while direct forms
such as `effigy graph` and `effigy docs` remain the only built-in execution
routes unless a later migration explicitly decides otherwise.

## Local Runtime And Providers

Effigy keeps local-runtime lifecycle, captured context, leases, aliases,
interrupt handling, route-table trust, and fail-closed behavior. Docker,
Colima, DNS, proxy, or later backend implementations may move behind provider
seams when the core lifecycle contract remains intact.

The façade may continue exposing a provider-backed operation. That reachability
does not transfer provider dependency or release ownership into core.

## Service Catalogs

Effigy owns catalog schema, deterministic layering, parameter resolution,
selection, extraction, override ownership, and assembly.

The concrete shipped service and workspace definitions may move to a separately
versioned default catalog pack. Ordinary commands must remain at least as
simple as they are now. The default pack must be available automatically,
support offline/bootstrap use, preserve project overrides, and expose missing
or unhealthy state through `doctor`. A mandatory manual install ceremony is
not an acceptable migration.

Every supported install keeps a compiled baseline pack permanently. It is the
automatic offline floor, including for `cargo install`, and remains available
for recovery after independently installed packs are introduced. Resolution is
project override, user override, active installed default pack, then compiled
baseline.

Independent pack releases use immutable OCI artifacts through Effigy's existing
artifact transport. Explicit local-path installation remains available for
development and recovery. Normal service, workspace, container, and task
commands never probe a registry or update a pack.

Installation is transactional: fetch or read the candidate, validate pack
schema and Effigy compatibility, write a versioned store entry, then atomically
activate it. A failed candidate leaves the previous active pack untouched. If
an active installed pack later becomes unreadable or incompatible, the resolver
uses the compiled baseline with a visible text warning and structured JSON
selection reason; `doctor` provides rollback or reset guidance.

The acquisition prototype does not prune installed pack content automatically.
It retains every successfully installed content entry while activation metadata
tracks the active and previous selections needed for deterministic rollback.
Garbage collection or a bounded retention policy requires a later explicit
operator decision; install, rollback, and reset never infer deletion authority.

The first implementation lane is an in-repository acquisition prototype. It
ships explicit digest-addressed OCI and local-path installation, status,
rollback, and reset while keeping today's embedded assets as the baseline. It
tests the fixed official-channel update planner but does not expose
argument-free `service pack update` before an official artifact exists. Pack
publication, concrete-asset movement, release/install wiring, and the live
no-argument update command remain a separate lane.

## Release And Distribution

Effigy core owns release readiness, exact-SHA identity, gate evaluation,
ordered mutation, irreversible-action safety, and evidence.

Effigy-specific repository, Homebrew, documentation, file, and self-hosting
recipes belong to this repository or an installed extension. Provider-neutral
distribution models may remain reusable libraries when independent consumers
and stable contracts justify them.

The `effigy release` façade may still orchestrate external recipes. Core
ownership stops at the generic transaction boundary.

## Rhai Storage And S3

Direct S3 CRUD is provider-specific and remains a future optional-provider or
removal candidate. It is not ready to move.

`bovine-accelerator` currently uses the Rhai storage surface for media upload.
`bovine-accelerator-desktop` is expected to assume that responsibility, but the
replacement is not live and proved. Preserve the existing S3 behavior until the
consumer migration satisfies contract `043`.

No deprecation, dependency removal, or base-Rhai removal may begin merely to
clean the Effigy dependency tree. The consumer path moves first; Effigy cleanup
follows proven replacement.

## Placement Inventory

| Capability | Placement |
| --- | --- |
| Task routing, execution, tests, health, init | Core |
| Manifest/config authoring and composition | Core façade plus reusable parsers |
| Environment, redaction, secret contracts | Core with optional backends |
| Local runtime lifecycle | Core façade with provider adapters |
| Catalog schema and assembly | Core |
| Concrete service/workspace definitions | Default optional asset pack candidate |
| Artifact, state, deploy, release transactions | Core provider-neutral transaction façades |
| Provider scripts, templates, and app hooks | Provider or consumer owned |
| Effigy self-distribution recipes | Repository or installed extension |
| Graph, scan, docs, contracts, papercuts | Core repository intelligence |
| Rhai execution boundary and typed Effigy routing | Core runtime contract |
| Standalone/provider-specific Rhai utilities | Optional runtime/provider candidates |
| External skill source code | Installed extension source behind core isolation |

## Sequencing

The feature-boundary follow-through is sequenced as separate lanes:

1. help-first command discovery with no execution aliases or removals
   (shipped by card `1093`);
2. release versus self-distribution separation;
3. catalog-pack acquisition prototype satisfying the simplicity invariant,
   followed separately by publication and concrete-asset movement;
4. repository-intelligence grouped discovery surface;
5. S3 migration only after the named consumer replacement proof.

These lanes should remain separate. None implies release work.

## Non-Goals

- optimize binary size;
- remove commands merely to reduce a count;
- add executable group namespaces in the help-first lane;
- require a plugin marketplace;
- remove S3 before consumer replacement;
- make catalog use more manual;
- extract provider-neutral repository intelligence;
- treat façade exposure as implementation ownership.

## Drift Triggers

Revisit this architecture when a new top-level family is proposed, a provider
dependency enters mandatory core, a grouped route diverges from its shortcut,
catalog acquisition adds operator ceremony, product-specific release defaults
enter reusable release code, or the S3 consumer dependency changes.
