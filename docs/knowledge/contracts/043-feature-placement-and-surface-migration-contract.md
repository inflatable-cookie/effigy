# 043 Feature Placement And Surface Migration Contract

Owner: product architecture, CLI routing, and extension boundaries
Created: 2026-08-31
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Research: `catalog-pack publication source map` (in Git history)

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

Help-first discovery groups the inventory without adding execution ceremony.
The executable namespace preview shipped and was then rejected by the operator;
direct invocation is canonical.

- `effigy --help` and `effigy help` group the general inventory by operator job.
- `effigy help <group>` accepts exactly `work`, `local`, `repo`, `deliver`,
  `extend`, or `admin` and renders that primary inventory.
- `effigy help <command>` renders the same help facts as
  `effigy <command> --help` through the existing typed help owner.
- An unknown help topic fails deterministically and points at valid groups and
  commands. It must not silently fall back to general help.
- Direct help keeps current deferral where repository routing gives a manifest
  selector precedence.
- Every general-help entry has exactly one primary group. Cross-links in detail
  help do not create a second primary entry or execution route.
- `local`, `repo`, `deliver`, `extend`, and `admin` remain help topics only.
  They are not built-in execution namespaces and do not reserve manifest task
  selectors.
- Every built-in uses its direct `effigy <command> ...` spelling. Arguments,
  side effects, exits, text, command identity, result payload, and error details
  retain one owner.
- General/group help preserves the primary topic taxonomy while showing direct
  spellings. Completion candidates use those direct spellings too.
- Direct detailed help carries no namespace-migration warning or removal fact.
- Existing repository task shadowing, slash selectors, and leading global flags
  retain their prior precedence.
- Genuine subcommands remain nested under their owning built-in; help taxonomy
  does not flatten domain grammar.

Primary ownership is fixed as follows:

| Topic | Primary commands and shapes |
| --- | --- |
| `work` (direct) | `<task>`, `<catalog>/<task>`, `tasks`, `draft`, `drafts`, `test`, `watch`, `doctor`, `init` |
| `local` | `container`, `system`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `graph`, `scan`, `docs`, `contracts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap`, `demo` |
| `extend` | `skill`, `rhai` |
| `admin` | `admission`, `config`, `deps`, `secrets`, `defer`, `uninstall`, `version`; `config completion` moves with `config` |

These names are help topics, not route prefixes. Canonical detail uses
`effigy <command> --help`; `effigy help <command>` remains available where
current deferral permits it. No multi-token
`effigy help <group> <command>` grammar is required by this contract.

`draft` and `drafts` are additive direct work commands governed by contract
[`046`](046-published-and-draft-task-surface-contract.md). They explicitly
select or inventory provisional task definitions; they do not turn the `work`
help topic into an executable namespace or change flat published-task routing.

## Repository-Intelligence Contract

Graph, scan, docs, and contracts remain provider-neutral core
capabilities.

The help group:

- points at the current direct implementations and output contracts;
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
- the prototype retains all successfully installed pack content and performs no
  automatic pruning; garbage collection or bounded retention remains a later
  explicit operator decision;
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

Publication and concrete-asset ownership are fixed as follows:

- canonical editable assets belong to
  `inflatable-cookie/effigy-catalog-pack` and use independent SemVer;
- the official OCI repository is
  `ghcr.io/inflatable-cookie/effigy-catalog-pack`;
- the failed pre-push annotated source tag `v1.0.0` remains immutable incident
  evidence; the first public release is `1.0.1`, with a separately reviewed
  annotated source tag and OCI version tag `v1.0.1`;
- Effigy carries a generated, pinned recovery snapshot with source commit, pack
  version, OCI manifest digest, and unpacked content identity evidence;
- every supported binary, Homebrew, source-build, `init`, and `bootstrap` path
  receives that compiled baseline without contacting GHCR or mutating active
  user pack state;
- ordinary service, container, system, workspace, and task commands never
  probe the registry or check channel freshness;
- only explicit immutable install or `service pack update` may acquire registry
  content, and update must resolve `stable` to a digest before entering the
  existing validate-store-activate transaction;
- `stable` moves only through protected manual dispatch against an existing
  annotated pack version tag; source and OCI version tags are process-immutable
  checked pointers, while the OCI manifest digest is the immutable identity;
- a failed source/version tag is never deleted, moved, or reused; repair ships
  under the next PATCH only after exact-head review and merge;
- the publication path requires digest-bound provenance, anonymous pull and
  exact-byte validation, an unchanged compatibility input, and a verified
  rollback target before channel promotion;
- a pack release may propose a generated baseline PR through a narrowly scoped,
  short-lived GitHub App credential, but it cannot approve, merge, or release
  Effigy;
- first publication and later Effigy release remain separate operator-gated
  mutations.

`stable` must remain compatible with every supported Effigy release that
publicly exposes `service pack update`. Raising that floor requires an explicit
support-policy change before channel movement; parallel compatibility channels
remain unplanned until a real floor change requires them.

### Canonical Source And Generated Snapshot

- The dedicated source repository has one canonical release root, `pack/`,
  containing top-level `pack.toml` and the exact catalog/support tree.
  Repository docs, workflows, and tooling stay outside it.
- Effigy's compiled baseline is a byte-for-byte generated copy of `pack/`.
  It is marked generated and direct edits fail repository checks.
- A typed sidecar lock records source repository, source commit, pack version,
  OCI manifest digest, and unpacked pack content identity.
- Offline QA recomputes manifest identity, version, and content identity from
  the snapshot and compares them with the lock without network access.
- Publication and baseline-proposal verification additionally pull by recorded
  digest, verify the digest-bound attestation, and compare exact paths and bytes.
- OCI manifest digest and unpacked content identity are distinct required facts;
  neither substitutes for the other.

### Effigy Compatibility Authority

Effigy owns `support/catalog-pack-update.toml`. Its versioned schema contains:

- `schema_version`;
- `as_of_release`;
- a nonempty, duplicate-free `required_versions` set containing the current
  release and every still-supported Effigy release that publicly exposes
  `service pack update`;
- optional `oldest_update_capable_release`, equal to the oldest required
  version once a public update-capable release exists.

Before the first update-capable release, `oldest_update_capable_release` is
absent and the current release still keeps the compatibility oracle non-vacuous.
Official artifact or channel publication does not, by itself, require that
field. It appears only once a released Effigy exposes public
`service pack update`. Only an Effigy support-policy or release PR may change
this file. Pack content, installed state, and the pack repository may consume
it but cannot redefine it. Effigy validates the file locally through one typed
parser; that validation does not contact the network and does not affect pack
selection, acquisition, or activation.

Before any package mutation, publication resolves the file from Effigy's current
default-branch commit, records that commit and the file blob digest, and proves:

- schema validity and internal oldest-version agreement;
- a GitHub Release exists for every required version;
- `as_of_release` equals Effigy's latest non-draft, non-prerelease release;
- the candidate pack compatibility range admits every required version.

The job resolves and rechecks the same authority before `stable` promotion.
Missing, empty, malformed, unresolvable, stale, inconsistent, changed, or
incompatible input stops the run with `stable` unchanged.

### Deterministic Publication And Update

- A protected manual dispatch accepts an existing annotated source `vX.Y.Z`
  tag and verifies its tag object, peeled commit, and manifest version.
- Source `v*` tags reject update and deletion. Neither routine maintainers nor
  the publication job have a bypass.
- Artifact construction fixes source bytes, path order, artifact type,
  configuration, annotations, and source-derived timestamp before computing a
  local OCI manifest digest.
- OCI `vX.Y.Z` absent permits creation at the candidate digest; the same digest
  permits idempotent retry; a different digest is a collision and stops without
  overwriting the pointer or moving `stable`.
- Artifact and tag writes belong only to protected publication jobs, serialized
  by version. For the first package, GitHub's documented operator package-
  settings control performs the explicitly authorized public-visibility change
  between the protected publish and finalize jobs; no undocumented REST PATCH
  is a release dependency. The OCI manifest digest is the immutable release
  identity and retry oracle; source and OCI version tags are process-immutable
  checked pointers.
- The finalize job re-resolves the version pointer, verifies public package
  linkage, attaches and verifies digest-bound provenance through exact-SHA
  `actions/attest`, pulls anonymously by digest, validates exact bytes and pack
  compatibility, fetches and rechecks Effigy's current support/release input,
  then moves and verifies `stable` at the same digest.
- When a previous verified `stable` digest exists, finalization exercises live
  retag rollback before restoring the candidate. When the first-publication
  target is absent, the non-mutating oracle proves rollback-to-absence and the
  live path moves `stable` once; it never deletes a manifest to imitate tag
  absence.
- A partial-push retry rebuilds the same candidate. It resumes only for an
  absent or same-digest remote version state; a changed source tag, changed
  deterministic input, or different-digest collision stops.
- `service pack update` reports the resolved channel and digest and sends only
  a digest-addressed candidate through the existing acquire-validate-store-
  activate transaction.
- An already-active digest that re-verifies is a deterministic no-op.
  Resolution, pull, compatibility, validation, or activation failure preserves
  active and previous local selections and channel metadata.

First publication remains an explicit operator-gated external mutation. Scoped
workflow implementation and a no-push rehearsal are authorized implementation
work; source-tag creation, package creation/visibility, channel movement, and an
Effigy binary release are not inferred from that authority.

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

## Rhai Storage Retirement

Tom's ruling (2026-10-01) authorized the retirement: "Effigy doesn't need to
wait for bovine, you can get on with removing those features from effigy
now". Effigy removed its Rhai `storage::*` host surface, the direct `s3`
dependency, and the vendored `vendor/s3` tree in one reviewed breaking change,
in parallel with Bovine's consumer cleanup and without a consumer-closeout
gate. The earlier replacement-upload and consumer-evidence requirements were
disapplied for this retirement by the same ruling; their history is in Git.
Residual callers (for example `bovine-accelerator` Rhai tasks) break by
design; their retirement is owned by the consumer repository, not by a
compatibility shim here.

The retirement removed only Effigy's old Rhai object-storage functions and
their vendored client library. It did not touch live objects, buckets,
secrets, or assets; generic MinIO catalog services and workspace-bundle
configuration remain; independent consumer S3 adapters (for example Farmyard
and Underlay) are unrelated infrastructure and keep their own owners. A
future optional-provider object-store surface remains unplanned: planning
must choose the transport and the minimum contract before any replacement
exists, and no cleanup may assume those choices.

## Migration Evidence

Every feature-placement migration must record:

- old owner, new owner, and retained Effigy invariant;
- current consumers and compatibility boundary;
- dependency and release-policy movement;
- direct command and help-group inventory parity where applicable;
- failure and repair behavior when an optional asset/provider is absent;
- docs, completion, agent-skill, JSON, and generated-reference impact;
- rollback or staged-removal path;
- focused and full Effigy validation.

## Stop Conditions

Stop and return to planning when:

- a façade route is used as the only proof of core ownership;
- help grouping needs a second command implementation or starts consuming
  selector grammar;
- an alias removal lacks explicit operator approval;
- catalog externalization adds mandatory operator ceremony or weakens offline
  behavior;
- release extraction weakens exact-SHA or irreversible-action safety;
- S3 removal precedes the consumer replacement gate;
- an extension transport or execution namespace spelling must be invented to
  proceed.

## Non-Goals

- removal of established direct commands;
- immediate S3 extraction;
- a second implementation behind help groups;
- a general plugin marketplace;
- binary-size optimization;
- release execution.

### Catalog v1.1.2 baseline import authority — 2026-10-08

Tom answered "Go for it" to the bounded compiled-baseline adoption proposal
(decision `4bda7fe2-7f5a-47a9-ae74-2a2699a1160f`). This authorizes one Queue
import PR, independent exact-head review and CI, merge/closeout, and one fresh
main QA milestone. It does not authorize a binary release, workflow or
provider mutation, global installation, or live container deployment.

The exact published artifact is
`ghcr.io/inflatable-cookie/effigy-catalog-pack@sha256:c7ed52c19ff498c0e4fe28eb33adf629fc96d9594962b95df3040b8a452c6d3d`,
source `bffcd0d8446286c49609c5b4659fe1d6f5d69c0f`, annotated `v1.1.2` tag object
`e0b8c8bdb74cccf7b61f6ff4b8ccf8b67f1c713a`, and unpacked content identity
`sha256:f65724ad5eef245fe4f2e6c3e831f1761b415daba8d8a5d4744d30341a6af022`.
Assessment verified all 42 files and 93,849 bytes. Compared with the current
v1.1.1 baseline, only `pack.toml` changes: version 1.1.2 and compatibility
`>=0.13, <0.15`; service assets remain byte-identical. All current supported
versions are admitted without changing the support floor.

The compiled snapshot and typed lock now track this artifact, materialized
from the anonymously pulled digest-addressed tree through the existing
generator. The raw manifest digest, registry descriptor, per-file layer
digests and sizes, source annotations, and digest-bound attestation subject
were checked against the supplied receipts. The generated tree contains 42
files and 93,849 bytes; only `pack.toml` differs from v1.1.1, while the other
41 service assets remain byte-identical. The lock and deliberate Rust test
pins record the v1.1.2 source commit, tag object, manifest digest, and content
identity.

The compatibility range `>=0.13, <0.15` admits supported Effigy versions
0.13.0, 0.13.1, 0.14.0, and 0.14.1, and rejects 0.12.1 and 0.15.0. The
fixed file-count, byte-count, identity, offline snapshot, and deterministic
regeneration proofs remain in place. Unrelated fixture provenance, the support
floor, active user pack selection, and explicit-update semantics are
unchanged.

### Underlay development profiles and host listeners — 2026-10-09

Tom authorized planning and promotion of the Underlay development-profile
work: "OK, that sounds doable. Let's plan it out and promote it."

The Underlay bundle owns its hybrid, container and short-lived test profile
recipes, including Vite and Rust service launch adapters, configurable
listeners, public URLs, service connection endpoints, browser origins and
HMR wiring. The intended consumer surface is `dev` for hybrid development,
`dev:container` for Linux runtime parity, and `dev:test` for disposable
worktree testing. These are bundle/consumer task selectors, not new built-in
Effigy commands. They are proposed behavior until independently qualified.

Effigy may own reusable language-agnostic runtime identity, environment,
readiness, interruption and gateway-route ownership invariants. It must not
recognize Vite, Cargo, Rust APIs or Underlay service conventions in core.
Rust compiler-cache management belongs to Oxide and is outside this work.

Concurrent host listeners require instance-specific addresses and actual
bind/readiness evidence before route publication. A free-port probe is not
a reservation. OS-assigned listeners may report their actual addresses;
adapters requiring an advance port need strict binding and bounded collision
handling. Gateway targets, application connection URLs and HMR endpoints
must agree with the actual listener. Restart and teardown may affect only
the recorded instance's processes and routes; ambiguous ownership remains
held rather than being reclaimed on age or PID disappearance.

The first qualification uses disposable fixtures to establish supported
interfaces and expose any missing generic prerequisite before dependent
bundle implementation. It covers simultaneous checkouts, occupied ports,
readiness, HTTPS/HMR routing, failed startup, restart and isolated teardown.
It must not introduce an unmanaged duplicate supervisor or assume a new
extension interface. Any generic Effigy prerequisite is a separate reviewed
task linked to its bundle dependants.

Because bundle consumers currently track `main`, profile additions preserve
existing defaults until an explicit consumer migration selects hybrid
development. Container parity remains available. This authority covers
bounded Queue briefs, independent review and qualification; it does not
authorize fleet configuration changes, live gateway or resolver mutation,
data deletion, workflow edits, binary publication or global installation.
