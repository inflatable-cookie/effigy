# 043 Feature Placement And Surface Migration Contract

Owner: product architecture, CLI routing, and extension boundaries
Created: 2026-08-31
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Research: `catalog-pack publication source map` (in Git history; superseded by Q-005)

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
| `work` (direct) | `<task>`, `<catalog>/<task>`, `tasks`, `runs` (with `run status\|logs\|stop`), `draft`, `drafts`, `test`, `doctor`, `init` |
| `local` | `container`, `system`, `workspace`, `gateway`, `service`, `exec` |
| `repo` | `graph`, `scan`, `docs`, `contracts` |
| `deliver` | `artifact`, `state`, `deploy`, `release`, `bundle`, `bootstrap` |
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
`runs` and `run` are direct work commands governed by contract
[`049`](049-heavy-validation-admission-contract.md); a root task named `run`
or `runs` owns that word, as for every built-in.

## Repository-Intelligence Contract

Graph, scan, docs, and contracts remain provider-neutral core
capabilities.

The help group:

- points at the current direct implementations and output contracts;
- explains which job each child surface performs;
- preserves standard leading `--repo` and `--json` behavior;
- does not add a second index, policy store, or refresh lifecycle.

## Catalog Source Contract

Effigy core keeps catalog schema, source layering, parameter resolution,
selection, extraction, fragment requirements, and assembly. Concrete
service/workspace definitions ship as the compiled baseline; projects, bundles
and third parties add or change services through catalog sources without an
Effigy release (Q-005, architecture
[026](../architecture/026-feature-placement-and-command-surface.md#service-catalogs)):

- resolution order is `project` (`infra/dev/catalog/`), `bundle` (the pinned
  bundle's `catalog/`), declared `[catalog.sources.<handle>]` in handle order,
  then `baseline`;
- `<source>/<service>` selects one source; `project`, `bundle` and `baseline`
  are reserved handles; catalog-keyed behaviour uses the unqualified name;
- declared git, OCI and path sources share the bundle source pin and
  acquisition code, one checkout lock, host Prepare refreshes and explicit
  `effigy bundle sync`;
  reads never fetch, and a guest reads the host's pins without holding its own
  catalog state;
- `service.toml` `[requires]` names the Effigy range, fragment schema and
  renderer capabilities a fragment needs, and is checked before the schema
  parse so an older release names what it lacks;
- `effigy service list` and `effigy container render` show each service's
  source.

The catalog-pack contract it replaces (pack store, `service pack`
install/update/rollback/reset/status, official OCI channel, workspace pack
handoff, `support/catalog-pack-update.toml`) is retired in v0.15. Installed
pack state under `~/.effigy/catalog-packs/` is ignored with a one-release
notice and left untouched. Pack v1.2.0, the last adopted baseline, was
byte-identical to the compiled catalog, which now stays in this repository as
ordinary source without a pack manifest or provenance lock.

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

### Catalog v1.2.0 baseline adoption — 2026-10-15

Tom approved publishing catalog pack v1.2.0 ("Happy for you to publish the
catalog pack") and chose to adopt it as Effigy 0.15's compiled baseline
("Adopt 1.2.0", v0.15.0 pre-release thread). This deliberately ships `silo` as a
built-in service; it does not record Silo acceptance for Underlay, which stays
with the bundle owner (contract 005).

The artifact is
`ghcr.io/inflatable-cookie/effigy-catalog-pack@sha256:7f34c21acce23033d5972868df13a9b061dab995b50b38adb954f7a1d1b5ef5b`,
source `588125bd68ad0c86106eb68b7c11d8c60be3fc86`, annotated `v1.2.0` tag
object `06704bdad8247ea2b6a414a77bf77bba58cf768e`, content identity
`sha256:539d63cf709110748902bd01a5cd6c5a359ca3636ffffaa87ddb95dff1125dd1`,
published with a digest-bound attestation and promoted to `stable`. The
snapshot is the anonymously pulled tree: 44 files and 95,527 bytes. Against
v1.1.2 it adds the `silo` service and changes `pack.toml` (version 1.2.0,
compatibility `>=0.13, <0.16`); every other asset is byte-identical. The range
admits 0.13.0 through 0.15.0 and rejects 0.12.1 and 0.16.0. The support floor,
active user pack selection and explicit-update semantics are unchanged.

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

#### Generic prerequisites and the designated pilot

Underlay Reference is the designated pilot, using disposable owned checkouts
and worktrees. Acowtancy is excluded until the solution is qualified and a
later adoption is explicitly arranged. Configuration/plan receipts and
synthetic listeners do not prove the actual Reference adapters ready.

Private gateway qualification requires a supported explicit mode that keeps
state and certificates in the caller-owned fixture, binds only unprivileged
loopback endpoints, and never installs trust, writes host resolver files,
adds host loopback aliases or escalates privilege. Clients verify TLS with
the fixture's explicitly supplied CA and resolve its names explicitly; a
private `CAROOT` alone does not isolate system trust. Defaults for ordinary
operator gateway setup retain their existing behavior. Isolation must cover
startup, daemon handoff, route changes, status and teardown, with unsafe or
ambiguous configuration refused before effects. Test-only interception of
the installer is not a supported consumer mode.

The supported Effigy surface is `gateway setup-tls --private-state-root
<DIR>` and `gateway up --private-state-root <DIR>` with optional loopback
address overrides. `EFFIGY_GATEWAY_PRIVATE_STATE_ROOT=<DIR>` carries the same
policy through normal container route registration and managed gateway
startup. The gateway reports its actual OS-assigned addresses and the fixture
CA path; clients supply that CA and resolve route names to the HTTPS address.
Private state validation currently requires Unix ownership and permission
checks, and fails closed on other platforms.
The implementation lives in `effigy-gateway` and the runner's gateway
command boundary; the CLI and container host integration carry the selection.
See the [private gateway guide](../../guides/063-container-system-guide.md#private-gateway-for-disposable-consumers)
and [gateway architecture](../architecture/020-container-infrastructure-design.md).
This capability does not qualify fixed-port consumer adapters or hybrid
child-listener ownership.

Managed host-listener support must reuse Effigy's existing managed-process
lifecycle and process identity primitives. Its language-neutral declaration
binds a child selector to instance-scoped listener discovery, readiness and
route intent. Discovery must prove the actual loopback socket belongs to the
recorded child generation; a requested port, child-written address or HTTP
response alone does not establish ownership. Endpoint projection to child
environment and sibling connection URLs, publication and restart replacement
must preserve this generation binding. Unknown ownership remains unavailable
and cannot authorize signals, route takeover or reclamation. Static
`target_host` configuration remains distinct from this managed ownership
guarantee and is not retroactively certified by it.

These prerequisites are independently reviewed implementation work within
the authorized profile lane. Private gateway isolation precedes integrated
listener/route qualification. The retained bundle qualification is evidence
input, not a completion prerequisite for the capabilities that unblock it.
No prerequisite implements Vite/Rust adapters, adopts a consumer, replaces
the host scheduler, changes its protocol, or implements the wider proposed
owned-run supervision contract merely to satisfy this local development use.

### Managed listener readiness diagnosis — 2026-10-09

Tom authorized continuing the bounded readiness follow-through after the
disposable Reference pilot reported an owned-listener startup timeout. The
next work qualifies the generic HTTP probe against ordinary server behavior
and preserves bounded, generation-bound phase evidence before cleanup. A
transport correction requires a demonstrated counterexample; the observed
pilot timeout alone does not establish its cause.

Diagnostic evidence must distinguish report discovery, socket ownership,
HTTP readiness and route publication without retaining credentials or
response bodies. It is evidence, never authority to publish, signal or
release resources. Existing identity, exclusivity, readiness deadlines,
private TLS and generation-safe teardown guarantees remain mandatory.
Framework adapters and fixture preconditions remain bundle-owned. This
authorization does not restart the consumer pilot, change a live runtime,
replace the installed binary or authorize a release.

## Wave-A retirements

These v0.15 cuts remove unused execution paths and inert configuration. Removed
command spellings refuse before executing effects; they are absent from help,
completion and the active payload/schema inventory.

| Removed surface | Replacement and migration |
| --- | --- |
| `effigy watch` | Invoke the underlying tool's native watch option, or run the task explicitly. Effigy owns no watch loop or watch lock. |
| `effigy tasks migrate` | Define `[tasks]` and `[test.suites]` explicitly. `effigy config` supplies examples; initialization does not import package scripts. |
| `EFFIGY_MANAGED_STREAM` | Unset it and use `--headless` or `EFFIGY_MANAGED_HEADLESS=1`. A set stream variable refuses managed execution for one release, including when `--headless` is also passed. |
| Transition-evidence receipts | No replacement writer. Current listener state and readiness diagnostics keep their existing roles. Historical receipt files remain untouched and are no longer read or appended. |
| `effigy gateway recover` and its hidden legacy protocols | The ≤0.13 recovery window is closed. Numeric-only records require host administrator verification and shutdown, confirmed absence, then removal of only the old PID/version pair. v0.14.1 identity-bearing gateway upgrades continue through `gateway up`/`down` with fresh-generation verification. |
| User-runtime design scaffold | Removed unused privilege inventory, readiness gate and migration snapshot helper. The separately implemented listener handoff was later retired in v0.15 (see below); no live migration was performed. |
| `effigy gateway setup-listener-handoff`, `listener-handoff-status`, `remove-listener-handoff` | No replacement. The gateway runs as your user: use `effigy gateway up` and inspect it with `effigy gateway status`. The three commands refuse with this text and the CHANGELOG entry; the definition generator, strict parser and launchd observation/removal plan are deleted. |
| `docs qualified` and contract 053 | Run repository-owned docs QA and the retired-concepts checker directly. There is no qualified private docs recipe or separate build profile. |
| `[manifest].root` | Remove it. Select the intended root explicitly with `--repo`; catalog membership stays explicit. Root and included fragments reject it with the source path. |
| User-global catalog override (`~/.effigy/catalog/`) | Move custom fragments into `infra/dev/catalog/` in each consuming repository. The old directory is ignored and left untouched, with one warning per catalog-using process for one release. |

Secret policy keys were excluded from this cut until consumer cleanup. Their
separate retirement is owned by
[contract 032](032-secret-and-local-config-management-contract.md#vault-policy-retirement).
The retained lifecycle lock and socket observations are separated from legacy
recovery code, without changing their authority. Gateway engine kernel adoption
is separate; this retirement adds no kernel API or parallel state authority.
