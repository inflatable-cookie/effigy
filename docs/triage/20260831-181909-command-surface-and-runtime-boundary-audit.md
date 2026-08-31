# Command Surface And Runtime Boundary Audit

Status: operator promotion discovery
Created: 2026-08-31
Updated: 2026-08-31
Owner: orchestrator; promotion remains operator-gated

## Observation

Effigy's public and internal feature surface has grown large. Much of that
breadth reinforces one operator entry point. Some implementations also force
provider or consumer concerns into Effigy's dependency, security, and release
tree without evidence that Effigy owns their semantics.

The concrete trigger is `effigy-rhai`'s S3 storage host API. It pulls `s3`
directly, and the workspace carries a patched `vendor/s3` copy for a transitive
security constraint. Direct object CRUD is useful, but usefulness and façade
exposure do not establish core ownership.

## Audit Question

Which capabilities belong to Effigy core, a reusable domain seam, an optional
runtime or provider, an installed extension, a consumer workflow, or a removal
lane?

The answer must preserve one obvious `effigy` entry where Effigy owns routing,
planning, safety, or transaction semantics. It need not preserve every current
implementation, dependency, or top-level command.

## Operator-Confirmed Decisions

Confirmed 2026-08-31:

- cleaner ownership is the primary outcome;
- command-surface coherence is the second concern;
- dependency-tree growth and release coupling are material concerns;
- binary size is not important and must not drive extraction;
- a broad Effigy façade is valuable when coherent, but no current top-level
  family is presumed permanent;
- semantic ownership is the core test: Effigy core owns deterministic routing,
  planning, safety, or transaction semantics even when a capability is not
  universal;
- façade exposure alone does not make a capability core;
- assume staged pre-`1.0` cleanup: recommend moves and removals now, then
  implement accepted changes through explicit migration lanes.

## Evaluation Criteria

Apply these tests in order:

1. Does Effigy own a deterministic routing, planning, safety, or transaction
   invariant that consumers should not recreate?
2. Is the implementation provider-neutral, or does one provider/runtime force
   its dependency and release policy into every Effigy build?
3. Can Effigy retain a coherent entry while implementation moves to a library,
   provider package, installed extension, or consumer task?
4. Is there current consumer evidence for the implementation, not only a
   plausible use case or historical note?
5. Does the current top-level name describe an operator job, or an
   implementation detail inside a broader family?

Binary size is excluded. Dependency ownership, security response, release
coupling, and duplicated policy remain relevant.

## Operator Promotion Checkpoint

Confirmed 2026-08-31 after planning intake:

- direct S3 CRUD currently supports media upload in `bovine-accelerator`; that
  functionality must remain available until a replacement is live and proved;
- `bovine-accelerator-desktop` is expected to assume media-upload ownership,
  but that handoff is not complete, so S3 extraction or base-Rhai removal is
  not implementation-ready;
- externalizing bundled catalog definitions is acceptable only if ordinary
  usage remains at least as simple as the embedded catalog is today;
- separating generic release orchestration from Effigy-specific distribution
  proof is accepted for canonical promotion.

Still operator-gated:

- command namespace changes require a concrete transition prototype before
  approval;
- repository-intelligence grouping requires a concrete command/help prototype
  before approval.

Current recommendation disposition:

1. S3 extraction: directionally valid, but blocked on live replacement and
   consumer migration proof. Preserve current behavior meanwhile.
2. Job-oriented namespaces: pending prototype and operator decision.
3. Optional catalog pack: accepted with a no-regression simplicity constraint;
   acquisition and update ergonomics must be designed before implementation.
4. Release/distribution split: accepted for promotion.
5. Repository-intelligence grouping: pending prototype and operator decision.

## Evidence Inventory

Current source and public-surface evidence:

- `effigy --help` and `crates/effigy-cli/src/command_surface.rs` expose 30
  top-level help routes, six named helper routes, and generic catalog/task
  selection.
- `effigy tasks` reports 29 built-in task families. Public CLI, manifest, and
  deferred built-in registries overlap but are not identical.
- `TaskManifest` exposes 23 root domains: catalog, bundle, defer, env, data,
  state, deploy, test, package manager, scan, shell, env schema, secrets, docs
  policy, task defaults, bootstrap, isolation, containers, systems,
  distribution, release, demos, and tasks. Several are one semantic cluster,
  not independent product boundaries.
- `effigy rhai surface --json` exposes 43 modules. Typed Effigy helpers route
  through core commands; lower-level filesystem, process, network, Git/forge,
  prompt, and storage helpers execute inside the embedded runtime.
- `crates/effigy-catalog/catalog` embeds 14 service/workspace definitions.
  Project and user overrides already prove a layered external-asset seam.
- state/deploy contracts already separate provider-neutral plans,
  transactions, safety gates, lineage, and reports from provider packages and
  app hooks.
- container/gateway contracts put runtime preparation, captured context,
  leases, aliases, interruption, route-table trust, and fail-closed behavior in
  Effigy while hiding Docker/Colima and gateway implementation details.
- release contracts make exact-SHA and irreversible transaction safety core.
  Distribution still contains Effigy-specific repository, Homebrew, docs,
  files, tasks, and self-hosting defaults.
- graph/scan/docs code is provider-neutral repository intelligence used for
  deterministic agent navigation and policy checks. Current `main` now includes
  the bounded `effigy docs context` retrieval contract and implementation from
  card `1089`.
- the graph refresh timed out at its documented 120-second budget and retained
  a stale index. Exact source reads and `rg` are authoritative for this packet;
  graph output is orientation evidence only.
- card `1089` landed during this audit. Its bounded docs-context retrieval
  shares codegraph ranking and versioned evidence contracts, strengthening the
  repository-intelligence grouping without changing its ownership result.

## Complete Classification Inventory

The placement column describes semantic ownership. It does not prescribe a
crate split or exact command spelling.

| Capability cluster | Current surfaces | Recommended placement | Boundary |
| --- | --- | --- | --- |
| Generic task runner | `<task>`, `<catalog>/<task>`, managed task helpers, `tasks`, `test`, `watch`, task defaults, isolation | Effigy core | Keep selector precedence, planning, concurrency, locks, cache, reports, and process lifecycle core. Task bodies remain consumer-owned. |
| Task/config authoring | `config`, `tasks migrate`, `tasks unlock`, `tasks cache`, completion, catalog and bundle manifest domains | Effigy core façade plus reusable parsers | Keep effective-manifest composition, previews, safe mutations, and selector generation core. Imported scripts and repository configuration remain consumer assets. |
| Setup and health | `help`, `version`, `doctor`, `init`, `uninstall` | Effigy core | These govern Effigy installation, diagnosis, scaffold safety, and owned-state removal. Templates can be versioned assets without changing ownership. |
| Environment and secrets | `secrets`, `defer`, `exec`, env, env schema, shell | Effigy core contract; optional secret backends | Keep schema validation, environment precedence, redaction, fallback routing, and execution safety core. External vault implementations should be adapters. |
| Dependency management | `deps`, package-manager manifest domain | Effigy core façade plus reusable domain seam | Keep planning, local-link safety, pin authorship, and reporting core. Cargo/Bun implementations need not define the permanent provider set. |
| Local runtime lifecycle | `container`, `system`, `workspace`, `gateway`, containers and systems domains | Effigy core façade; optional runtime providers | Keep lifecycle, captured context, leases, aliases, interrupts, routing trust, and fail-closed semantics core. Docker, Colima, DNS, and proxy implementations can sit behind provider seams. |
| Service assembly and catalog | `service`, catalog domain, 14 embedded definitions | Core catalog schema/selection; optional catalog pack | Keep deterministic layering, parameter resolution, extraction, and override ownership core. Move shipped service/workspace definitions toward a separately versioned optional asset pack. |
| Acquisition and composition | `bootstrap`, `bundle`, bootstrap and bundle domains | Effigy core orchestration; optional source adapters | Keep clone/update planning, provenance, staged application, child ordering, and safety core. Repository recipes and source-specific assets remain external. |
| Artifact and data operations | `artifact`, data domain | Core transaction façade plus reusable domain crates | Keep typed plans, staging, provenance, digest/report contracts, target resolution, and handoff safety core. Database commands and payload semantics belong to adapters or consumers. |
| State and deploy | `state`, `deploy`, state and deploy domains | Effigy core transaction façade; external provider packages | Keep provider-neutral models, safety gates, lineage, reports, and transaction sequencing core. Provider scripts/templates/actions and app hooks remain external. |
| Release | `release`, release domain | Effigy core transaction façade | Keep readiness, exact-SHA gates, ordering, irreversible-action safety, and evidence core. Project-specific version/changelog/files/gates remain manifest or consumer policy. |
| Distribution and changelog | distribution built-in/domain; changelog helper surfaces | Installed extension or consumer workflow, with reusable libraries where proven | Effigy's own Homebrew/docs/files/self-hosting proof is consumer-specific. Retain only generic planning/safety reached through release; move project proof and publishing recipes out of the mandatory core release tree. Treat standalone changelog work as release/project maintenance, not an equal product domain. |
| Repository intelligence | `graph`, `scan`, `docs`, `contracts`, `papercuts`, scan and docs-policy domains | Effigy core | Deterministic repository discovery, policy validation, and agent navigation are core product semantics. Group the entry surface without extracting the implementations merely because they are not daily commands. |
| Demo proof | `demo`, demos domain | Core lifecycle/report façade; consumer-owned definitions | Keep proof-state discovery, execution lifecycle, and report contracts core. Demo scenarios, scripts, and expected product behavior belong to the repository. |
| Embedded Rhai runtime | `rhai surface` and 43 host modules | Optional runtime with a minimal core execution contract | Keep script loading, typed Effigy routing, capability disclosure, error/redaction, and execution boundaries. Split provider-specific and standalone utility modules into optional packs when they add independent dependency/security policy. |
| Direct S3 object CRUD | Rhai `storage::{provider,status,ls,head,get,put,delete}` | Optional provider/extension; base-Rhai removal candidate | No core transaction currently requires direct object CRUD. Extraction may remove these functions from base Rhai after an explicit migration lane. |
| External skill execution | `skill` | Effigy core isolation/dispatch façade; installed extension sources | Keep explicit source/target separation, host-only rejection, consumer-repo routing, and failure semantics core. Skill task bodies and provider logic stay external. |

No whole cluster above is recommended for immediate deletion. Removal
candidates are duplicate top-level placements and provider-specific functions
whose semantic owner moves outside base Effigy.

## Namespace Model Direction

Adopt an explicit job-oriented namespace model. Do not freeze exact namespace
names in this packet. Promotion should settle grammar and names after current
help-surface drift is reconciled.

Direction:

- keep the daily runner spine short and directly reachable: generic task
  selection, task inventory, tests, health, and initialization;
- group local environment work: containers, systems, workspaces, gateways,
  services, and ad-hoc execution share one lifecycle context;
- group repository intelligence: graph, scan, docs, contracts, and papercuts
  share one repository-analysis context;
- group delivery and state transitions: artifact, state, deploy, release,
  bootstrap, and bundle need a coherent transaction-oriented layout, even if
  acquisition later proves worthy of a separate group;
- place runtime and extension administration together: skill execution, Rhai
  surface inspection, and future optional provider discovery;
- subordinate changelog and distribution work to release or project
  maintenance instead of preserving equal top-level status;
- preserve façade reachability for deploy/container implementations only while
  Effigy owns lifecycle, safety, or transaction semantics. A façade route must
  not be used to claim ownership of provider code.

Migration should be a staged pre-`1.0` lane: publish the target map, add the
new routes and focused diagnostics, update examples and generated help, then
remove accepted legacy routes at an explicitly chosen milestone. Exact aliases,
warning duration, and removal versions remain promotion decisions.

## Pressure-Point Deep Dives

### 1. Rhai storage and S3

Evidence:

- `crates/effigy-rhai/src/host_api/storage.rs` registers
  `storage::provider`, `status`, `ls`, `head`, `get`, `put`, and `delete`
  unconditionally.
- runtime configuration rejects every provider except `s3` and defaults to AWS
  S3 endpoints.
- `effigy-rhai` directly depends on `s3 = 0.1.36`; `cargo tree -p
  effigy-rhai -i s3` identifies it as the only Effigy owner.
- root `[patch.crates-io]` replaces `s3` with `vendor/s3` because upstream
  constrained `quick-xml` to a vulnerable line.
- the helper shipped in `0.8.5`. Current repository evidence shows maintenance
  cost, but no broad consumer proof for mandatory direct object CRUD.

Recommendation: extract standalone S3 CRUD to an optional provider/extension
surface. It may leave base Rhai. Retain a typed object-store façade only if a
core artifact, state, or deploy transaction later requires it. Remove the
mandatory dependency and vendored patch only through an explicit migration and
security-validation lane.

Alternative: keep S3 in base Rhai because object storage is common. This is
weaker under the ownership test unless consumer evidence shows direct CRUD is
part of an Effigy-owned transaction rather than script convenience.

### 2. State and deploy

Evidence: architecture and contracts already put provider-neutral plan
derivation, safety gates, report persistence, state lineage, and transaction
ordering in core. Provider packages own scripts, templates, and actions. App
hooks own transforms, media semantics, and conflict resolution.

Recommendation: preserve that boundary. Deploy implementations may remain
reachable through the Effigy façade only where Effigy owns lifecycle, safety,
or transaction semantics. Keep provider packages independently versionable and
do not pull app behavior inward.

Alternative: extract deploy entirely because providers are external. This
would duplicate transaction and safety semantics across consumers and fails the
semantic ownership test.

### 3. Containers, gateway, and service catalogs

Evidence: core currently owns runtime preparation, captured context,
`ContainerManager`, lifecycle/lease/alias/interrupt semantics, and privileged
route-table trust. Docker/Colima and gateway details sit behind those
contracts. The catalog separately embeds 14 concrete service/workspace
definitions while supporting project and user overrides from disk.

Recommendation: retain the runtime manager and façade in core. Move concrete
backends only when their adapter seam preserves core lifecycle and safety.
Move the 14 shipped definitions toward a versioned optional catalog pack; keep
schema, deterministic selection, assembly, and override rules core.

Alternative: keep the catalog embedded for zero-setup use. This remains viable
if promotion accepts synchronized Effigy releases as the desired catalog
distribution model.

### 4. Release and distribution

Evidence: release contracts own exact-SHA verification, gates, ordered
mutation, evidence, and irreversible transaction safety. Distribution defaults
still name Effigy's repository, Homebrew formula, docs, files, tasks, and
self-hosting proof.

Recommendation: keep the generic release transaction in core. Extract Effigy
self-distribution proof and publishing recipes to an installed extension or
repository-owned tasks. Retain reusable distribution planning only where it
has provider-neutral contracts and independent consumers.

Alternative: retain distribution in core but remove all Effigy-specific
defaults and require explicit manifest configuration. This reduces coupling
without creating an extension transport first.

### 5. Graph, scan, and docs policy

Evidence: these surfaces provide deterministic code navigation, repository
scanning, contract checks, and docs policy for agents and CI. Their dependency
tree is implementation-heavy but not tied to a deployment or release provider.

Recommendation: keep the repository-intelligence stack in core and consolidate
its operator entry under the namespace model. Do not use low invocation
frequency as evidence for extraction.

Alternative: ship intelligence as an optional binary. That would reduce the
base dependency tree, but it would split a strategic, provider-neutral Effigy
contract without resolving an ownership problem.

## Ranked Recommendations

1. Extract direct S3 CRUD from mandatory base Rhai into an optional
   provider/extension surface. Treat base-Rhai removal as an explicit migration
   lane, not an incidental refactor.
2. Define and promote the job-oriented namespace model, then stage route moves
   before `1.0`. Start with the local-runtime cluster because its current
   container/system/workspace/gateway/service/exec overlap is clearest.
3. Externalize the 14 shipped service/workspace definitions as a versioned
   optional catalog pack while retaining catalog schema, layering, assembly,
   and override ownership in core.
4. Separate generic release transaction semantics from Effigy-specific
   distribution proof. Move self-hosting publication recipes to an installed
   extension or repository-owned tasks.
5. Consolidate graph/scan/docs/contracts/papercuts as one core repository-
   intelligence entry surface. Preserve their provider-neutral implementations
   unless later evidence identifies an actual ownership or release-policy
   conflict.

These are audit recommendations, not approved implementation order. Promotion
must turn accepted items into separate, bounded migration lanes.

## Alternatives Considered

- Keep the flat façade and change only crate internals. This avoids command
  churn but leaves operator grouping and top-level ownership signals unclear.
- Make every non-universal capability a separate binary. This mistakes
  universality for ownership and fragments core safety/transaction semantics.
- Keep provider implementations in core whenever the façade exposes them. This
  preserves convenience but makes routing equal ownership, contrary to the
  confirmed test.
- Retain S3 until an object-store abstraction supports multiple providers. This
  delays churn but keeps an unowned dependency and vendored security burden in
  the mandatory release tree.
- Keep bundled catalogs and distribution defaults for zero-setup use. This is
  coherent only if synchronized Effigy releases are deliberately accepted as
  their product lifecycle.

## Unresolved Questions

- What exact namespace grammar and names best preserve discoverability? This
  packet sets clusters, not spellings.
- Which extension transport should optional providers use: installed skill,
  provider package contract, feature-gated companion, or a new transport?
- How long should legacy command aliases and base-Rhai compatibility last, and
  at which pre-`1.0` milestone should removal occur?
- What evidence threshold is sufficient to move a provider implementation into
  mandatory core: two independent consumers, one core transaction need, or a
  different test?
- What is the minimum base Rhai host surface after provider-specific helpers
  move?
- What exact evidence proves `bovine-accelerator-desktop` has replaced the
  media-upload path and makes removal safe for `bovine-accelerator`?
- Should the optional service catalog pack ship beside the default installer,
  or require an explicit acquisition step?

## Non-Goals

- binary-size optimization;
- code, crate, Cargo, or vendored-dependency changes in this packet;
- exact namespace names or command syntax;
- a general plugin marketplace design;
- automatic extraction based only on dependency count;
- roadmap compilation, compatibility implementation, release work, promotion,
  or merge;
- treating fewer commands as inherently better.

## Suggested Promotion Map

After accepted review and merge, the orchestrator should reconcile current
`main` and choose canonical destinations:

- semantic core and placement criteria -> product guardrails and architecture
  overview, with a focused feature-placement contract if needed;
- namespace principles and compatibility posture -> command-surface contract
  plus later strict migration cards;
- Rhai/S3 placement -> embedded-runtime architecture/contract and a separate
  extraction lane;
- catalog core versus optional definitions -> catalog architecture and
  distribution guidance;
- release versus self-distribution -> release/distribution architecture and
  transaction contracts;
- repository-intelligence grouping -> codegraph/docs architecture, reconciled
  with the landed bounded docs-context contract.

Merge of this packet would be intake only. It must not make any recommendation
execution-ready without separate promotion and readiness work.

## Next Task

The orchestrator reviews the planning PR at its exact head for evidence,
scope, drift, and separation of confirmed decisions, recommendations,
alternatives, and unresolved questions. Do not promote or merge from this
delegate lane.
