# Feature Placement And Command Surface Architecture

Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)
Research: `catalog-pack publication source map` (in Git history; superseded by Q-005)

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

Help-first discovery groups the broad inventory without changing execution.
The later executable-namespace preview proved too heavy in normal use. Direct
built-in invocation is the canonical operator grammar.

1. `<task>`, `<catalog>/<task>`, and built-ins all remain direct.
2. `help`, `--help`, `--version`, leading `--json`, and `--repo` remain direct.
3. `local`, `repo`, `deliver`, `extend`, and `admin` are help categories, not
   executable built-in namespaces or reserved selector words.
4. `effigy --help` and `effigy help <group>` preserve job-based discovery while
   displaying direct command spellings.
5. Completion, current guides, generated references, and managed skills teach
   direct spellings.
6. Repository task shadowing and slash-selector precedence remain unchanged.
7. Genuine command-owned subcommands stay nested under their owning command.

The exact primary help taxonomy is:

| Topic | Primary commands and shapes |
| --- | --- |
| work | `<task>`, `<catalog>/<task>`, `tasks`, `test`, `doctor`, `init` |
| `local` | `container`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `scan`, `docs`, `contracts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap` |
| `extend` | `skill`, `rhai` |
| `admin` | `config`, `deps`, `secrets`, `defer`, `uninstall`, `version`; `config completion` moves with `config` |

The topic names organize help only. Each listed built-in executes as
`effigy <command> ...` and retains one implementation and one output owner.
Detailed help may cross-link borderline capabilities such as `bootstrap`
or `secrets` without creating another primary route.

## Repository Intelligence

Scan, docs, and contracts remain Effigy core. They provide provider-neutral,
deterministic policy evidence for operators, agents, and CI. v0.15 removed the
code graph (`effigy graph`, `effigy docs context`) and the three graph-backed
scanners; `rg` plus the `docs/README.md` index carry code and documentation
navigation.

Grouping improves discovery; it does not become execution grammar or justify a
second implementation. `effigy scan` and `effigy docs` are the canonical routes
and keep current manifest deferral behavior.

## Local Runtime And Providers

Effigy keeps local-runtime lifecycle, captured context, leases, aliases,
interrupt handling, route-table trust, and fail-closed behavior. Docker,
Colima, DNS, proxy, or later backend implementations may move behind provider
seams when the core lifecycle contract remains intact.

The façade may continue exposing a provider-backed operation. That reachability
does not transfer provider dependency or release ownership into core.

## Service Catalogs

Effigy owns catalog schema, source layering, parameter resolution, selection,
extraction, fragment requirements, and assembly. Projects, bundles and third
parties add or change services, workspace images and configuration without an
Effigy release by publishing fragment directories as catalog sources. Pinned
source is enough; there is no central registry (Q-005).

### Catalog Sources

A service resolves from the first source that provides it, in this order:

1. `project`: the repository's committed `infra/dev/catalog/` directory, plus
   any `compose_file` service read as a project fragment for its one-release
   window;
2. `bundle`: the `catalog/` directory shipped inside the pinned bundle;
3. declared sources: `[catalog.sources.<name>]` git, OCI or path sources,
   resolved in handle order;
4. `baseline`: the catalog compiled into the binary.

A name qualified as `<source>/<service>` (for example `baseline/redis` or
`acme/queue`) resolves from that source only. Behaviour that keys on a catalog
entry (workspace identity defaults, database engines, PHP host integration)
uses the unqualified service name. `project`, `bundle` and `baseline` are
reserved handles. `effigy service list` names each service's winning source,
its path and the sources it shadows; `effigy container render` names the source
of each environment service. Shared services are machine-scoped and resolve
from the compiled baseline only.

Declared sources share the bundle source pin and acquisition code: one
checkout lock (`.effigy/sources.lock.json`) records the bundle and every
`catalog:<name>` pin, and a host Prepare (`dev`, `bootstrap`, `container up`)
or an explicit `effigy bundle sync` moves
them. Resolution reads the pinned local snapshots and never fetches; an
unprepared declared source is an error that names `effigy bundle sync`.

### Fragment Requirements

A fragment may declare what it needs in `service.toml`:

```toml
[requires]
effigy = ">=0.16"
schema = 2
capabilities = ["loopback-alias"]
```

Requirements are read before the rest of the schema, so a release that cannot
render a fragment refuses it and names the Effigy range, fragment schema and
capabilities it lacks, rather than failing on unknown fields. An unknown
requirement kind is itself unmet. This release renders fragment schema 1.

### Host-Only Resolution

Acquisition is host authority. Source pins are written only on the host by
`effigy bundle sync`, which refuses inside a workspace container. A guest
Effigy reads the same pinned snapshots through the checkout mount and never
holds its own catalog state: nothing is copied or installed into a guest home.

### Retired Catalog-Pack Runtime

The installed catalog-pack runtime is retired in v0.15: the pack store,
`service pack install/update/rollback/reset/status`, the official OCI
channel, the workspace pack handoff, and the update support policy
(`support/catalog-pack-update.toml` and `release.sync-catalog-pack-support-policy`).
Its fragments were already byte-identical to the compiled baseline, which is
now an ordinary Effigy-owned source tree rather than a vendored pack snapshot.
Existing `~/.effigy/catalog-packs/` state is ignored with a one-release notice
and left untouched. Effigy no longer reads the pack repository; archiving it is an operator action.

## Release And Distribution

Effigy core owns release readiness, exact-SHA identity, gate evaluation,
named GitHub Actions hosted-evidence reuse, ordered mutation,
irreversible-action safety, and evidence.

Effigy-specific repository, Homebrew, documentation, file, and self-hosting
recipes belong to this repository or an installed extension. Provider-neutral
distribution models may remain reusable libraries when independent consumers
and stable contracts justify them.

The `effigy release` façade may still orchestrate external recipes. Core
ownership stops at the generic transaction boundary.

## Rhai Storage And S3

Effigy's Rhai object-storage surface and its vendored S3 client library were
removed as one breaking change recorded in
[contract 043](../contracts/043-feature-placement-and-surface-migration-contract.md)
under Tom's independent-retirement ruling. Scripts that still call the removed
`storage::*` helpers fail with an unknown-function error; no compatibility
shim exists. Live objects, buckets, secrets, and assets were untouched;
generic MinIO catalog services and independent consumer S3 adapters remain
outside Effigy's ownership. A future optional-provider object-store surface
stays unplanned until its transport and minimum contract are chosen.

## Placement Inventory

| Capability | Placement |
| --- | --- |
| Published/draft task routing, execution, tests, health, init | Core |
| Manifest/config authoring and composition | Core façade plus reusable parsers |
| Environment, redaction, secret contracts | Core with optional backends |
| Local runtime lifecycle | Core façade with provider adapters |
| Catalog schema and assembly | Core |
| Concrete service/workspace definitions | Compiled baseline plus project, bundle and declared catalog sources |
| Artifact, state, deploy, release transactions | Core provider-neutral transaction façades |
| Provider scripts, templates, and app hooks | Provider or consumer owned |
| Effigy self-distribution recipes | Repository or installed extension |
| Graph, scan, docs, contracts | Core repository intelligence |
| Rhai execution boundary and typed Effigy routing | Core runtime contract |
| Standalone/provider-specific Rhai utilities | Optional runtime/provider candidates |
| External skill source code | Installed extension source behind core isolation |

## Sequencing

The feature-boundary follow-through is sequenced as separate lanes:

1. help-first command discovery with no execution aliases or removals
   (shipped by card `1093`);
2. release versus self-distribution separation;
3. catalog-pack acquisition prototype, since replaced by catalog sources and
   retired in v0.15 (Q-005);
4. executable grouped-command preview with retained direct migration aliases
   (shipped by card `1109`, then rolled back by card `1110`);
5. S3 migration only after the named consumer replacement proof.

These lanes should remain separate. None implies release work.

## Non-Goals

- optimize binary size;
- remove commands merely to reduce a count;
- reopen executable help-taxonomy aliases or reserve help-group words;
- require a plugin marketplace;
- remove S3 before consumer replacement;
- make catalog use more manual;
- extract provider-neutral repository intelligence;
- treat façade exposure as implementation ownership.

## Task visibility

Architecture [028](028-published-and-draft-task-surfaces.md) keeps public and
internal discovery on one task namespace and execution path. The grouped
human inventory and complete machine inventory share effective metadata.
Contract [046](../contracts/046-published-and-draft-task-surface-contract.md)
owns the grammar, defaults and one-release syntax migration.

## Drift Triggers

Revisit this architecture when a new top-level family is proposed, a provider
dependency enters mandatory core, catalog acquisition adds operator ceremony,
product-specific release defaults enter reusable release code, task identity or visibility changes, or the S3 consumer dependency changes.
