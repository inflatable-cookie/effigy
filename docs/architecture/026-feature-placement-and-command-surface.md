# Feature Placement And Command Surface Architecture

Status: active
Updated: 2026-09-01
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)
Research: [`catalog-pack publication source map`](../research/source-hubs/002-catalog-pack-publication-source-map-v1.md)

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

The acquisition prototype shipped explicit digest-addressed OCI and local-path
installation, status, rollback, and reset while keeping today's embedded assets
as the baseline. Official publication is a separate ownership boundary:

- canonical editable assets live in
  `inflatable-cookie/effigy-catalog-pack`, not in Effigy core;
- pack releases use independent SemVer and publish to
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`;
- Effigy keeps a generated, pinned recovery snapshot with source commit, pack
  version, and published OCI digest evidence;
- the compiled snapshot supplies automatic availability to every supported
  binary, Homebrew, source-build, `init`, and `bootstrap` path without a
  registry probe or implicit installed-pack activation;
- registry acquisition remains explicit through immutable digest install or
  `service pack update`;
- `stable` moves only through a protected manual dispatch for an existing
  annotated pack version tag; the OCI manifest digest is the immutable
  publication identity.

The pack repository owns validation, publication, provenance, and generated
baseline proposals. Effigy independently accepts or rejects each proposed
snapshot and retains its own release authority. Public `service pack update`
stays unavailable until an anonymously readable, digest-bound, attested
official artifact can succeed from the first exposed release.

### Canonical Pack And Generated Baseline

The dedicated source repository owns one canonical asset root: `pack/`. Its
top-level `pack.toml` and the exact catalog/support tree are release input;
repository documentation, workflows, and tooling remain outside that root.

Effigy's compiled baseline is a generated byte-for-byte copy of `pack/`, not a
second editable authority. A typed sidecar lock records the source repository,
source commit, pack version, OCI manifest digest, and unpacked pack content
identity. Repository markers and QA reject direct edits.

Drift proof has two layers:

- offline Effigy QA recomputes manifest identity, version, and content identity
  from the checked-in snapshot and compares them with the lock;
- publication and baseline-proposal proof pulls the artifact by recorded digest,
  verifies its digest-bound attestation, and compares the exact paths and bytes.

The OCI manifest digest and unpacked content identity remain separate evidence.
The former owns registry transport identity; the latter owns the extracted tree.

### Compatibility And Publication Authority

Effigy owns `support/catalog-pack-update.toml`, the machine-readable compatibility
set for the public update channel. It records a schema version, the release at
which the policy was checked, every still-supported Effigy version that exposes
update, and—once a released Effigy exposes public update—the oldest such version.
Official artifact or channel publication does not, by itself, introduce that
oldest field. Only an Effigy support-policy or release change may alter the
file. The pack repository consumes it from a resolved Effigy default-branch
commit and cannot redefine it. `effigy-catalog` validates the file locally and
network-free. That parser is not on the pack selection, acquisition, or
activation path.

Pack publication is deterministic and process-immutable. A protected manual
dispatch builds a local OCI layout from fixed source bytes and metadata,
computes the candidate manifest digest, and treats the remote `vX.Y.Z` state as
absent, an idempotent same-digest retry, or a blocking collision. It proves the
version pointer, digest, attestation, anonymous pull, exact bytes, and fresh
Effigy compatibility input before moving `stable` to that digest.

Source and OCI version tags are checked pointers, not immutable identities.
Source `v*` tags reject update and deletion with no routine maintainer or
publication-job bypass. Package write authority belongs only to the protected
publication job, serialized by version. The OCI manifest digest is the retry
oracle and release identity.

`service pack update` reports the resolved channel and digest. A verified
already-active digest is a deterministic no-op. Resolution, pull,
compatibility, validation, or activation failure preserves active and previous
selection plus channel metadata.

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

Preservation includes bounded safety repairs required by a current consumer.
Contract `044` adds atomic create-if-absent behavior to the retained Rhai PUT
surface after Bovine proved HEAD then PUT cannot prevent overwrite races. That
repair neither promotes S3 to permanent core nor supplies removal evidence.

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
   followed separately by dedicated-repository publication, generated-baseline
   cutover, and public update exposure;
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
