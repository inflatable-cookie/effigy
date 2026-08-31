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

The approved direction is group-first and aliases-stable.

1. `effigy --help` presents commands by operator job instead of one flat list.
2. Grouped routes may provide a coherent discovery path.
3. Existing direct commands remain valid shortcuts when grouped routes arrive.
4. A grouped and direct route must resolve to the same typed request, behavior,
   output contract, and exit semantics.
5. Adding a group does not approve deprecation. Each alias removal needs its own
   explicit pre-`1.0` migration decision and evidence.

The initial conceptual groups are:

- daily work: selectors, tasks, tests, doctor, and init;
- local runtime: container, system, workspace, gateway, service, and exec;
- repository intelligence: graph, scan, docs, contracts, and papercuts;
- delivery and state: artifact, state, deploy, release, bundle, and bootstrap;
- extension administration: skill, Rhai, and future optional providers.

Exact namespace names and grammar remain a decision-prototype question. The
architecture approves the grouping model, not a wholesale rename.

## Repository Intelligence

Graph, scan, docs, contracts, and papercuts remain Effigy core. They provide
provider-neutral, deterministic repository navigation and policy evidence for
operators, agents, and CI.

Grouping improves discovery; it does not justify a second implementation or an
optional binary. Existing direct forms such as `effigy graph` and `effigy docs`
remain stable unless a later migration explicitly decides otherwise.

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

The pack acquisition and update mechanism is not selected yet.

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

The next planning batch may compile separate migration lanes only after the
active documentation-profile card `1090` settles:

1. command/help grouping prototype with alias parity and no removals;
2. release versus self-distribution separation;
3. catalog-pack acquisition prototype satisfying the simplicity invariant;
4. repository-intelligence grouped discovery surface;
5. S3 migration only after the named consumer replacement proof.

These lanes should remain separate. None implies release work.

## Non-Goals

- optimize binary size;
- remove commands merely to reduce a count;
- select exact namespace spellings in architecture;
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
