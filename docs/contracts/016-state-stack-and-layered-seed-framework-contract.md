# 016 - State Stack And Layered Seed Framework Contract

Status: Active
Owner: Platform
Created: 2026-05-08

## Purpose

Effigy needs a standard framework for how a repo composes system state from
schema, baseline seed data, imported snapshots, overlays, and later captures.

The shipped artifact substrate already covers transport, staging, metadata, and
OCI/local parity. This contract sits above that substrate and defines the
ordered replay model.

The first proof target is Acowtancy's legacy-to-new-site UAT loop, where:

- legacy data arrives through replayable OCI artifacts
- new-system-authored UAT changes must be captured without being lost
- refreshes from the still-live legacy site must be able to feed a rebuilt
  baseline later

Effigy owns the framework for that loop. The app still owns what the data means
and how conflicts are resolved.

## Scope

The state-stack framework owns:

- lifecycle phase taxonomy for seeded system state
- stack-manifest structure and ordered layer replay
- separation between layer orchestration role and artifact payload kind
- runtime plan/report shape for stack apply/capture operations
- lineage/provenance records for applied stacks
- app-hook boundaries for role-specific apply/capture work
- explicit UAT freeze/capture/rebase workflow support

The state-stack framework does not own:

- source-to-target transform logic
- schema-specific or record-level merge decisions
- domain validation
- media rewrite semantics
- post-go-live sync engines
- automatic background replication

## Core Terms

State stack:

- the ordered set of layers used to build or rebuild an environment

Layer:

- one declared state input in a stack
- may resolve from a repo-local path, an OCI artifact, or an app-produced
  capture output

Layer role:

- the orchestration meaning of a layer
- examples: `structure`, `baseline-seed`, `legacy-import`, `overlay`,
  `dev-overlay`, `uat-capture`, `full-capture`

Artifact kind:

- the coarse descriptive type of a payload
- examples: `sql-dump`, `migrated-base-snapshot`, `content-overlay`,
  `app-specific`

Layer role and artifact kind are different:

- role says where a layer sits in the replay lifecycle
- kind says what sort of payload was packaged

Lineage:

- the immutable record of what exact stack, layer refs, digests, and hooks were
  used to build an environment

Rebase:

- a deliberate rebuild path where newer imported legacy state and captured
  new-system-authored changes are reconciled into a new baseline

## Phase Taxonomy

Effigy recognizes this first phase model for the first shipped stack surface.

### 1. `structure`

- repo-committed schema baseline
- usually SQL migrations or equivalent schema-apply contract

### 2. `baseline-seed`

- repo-committed low-volatility data
- examples: roles, groups, lookup rows, platform invariants

### 3. `legacy-import`

- imported state derived from an external or legacy source
- usually replayable through local or OCI artifacts
- Effigy does not define the transform logic that produced it

### 4. `base-apply`

- the point where imported layers are applied onto a clean target system

### 5. `dev-overlay`

- optional local or non-production-only fixture layer
- must never be a dependency of imported baseline data

### 6. `working-baseline`

- checkpoint label, not necessarily a separate payload
- means the system is now usable for UAT or operator review

### 7. `uat-capture`

- captured changes authored inside the new system after the baseline was built

### 8. `legacy-refresh`

- newer imported legacy layer generated from a later source snapshot

### 9. `rebase`

- offline reconciliation plus rebuild path that combines captured new-system
  changes with refreshed imported legacy state into a new baseline

### 10. `schema-evolution`

- ordinary forward schema changes after the baseline already exists

### 11. `full-capture`

- whole-system capture for replay elsewhere

This taxonomy is ordered guidance, not a claim that every repo uses every phase.

First implementation rule:

- implementation may model every role now, but it should only execute
  `structure`, `baseline-seed`, `legacy-import`, `dev-overlay`,
  `uat-capture`, and `full-capture` until refresh/rebase command semantics are
  promoted by a later card

## Source Rules

Layer sources may be:

- repo-local path
- explicit `oci://` artifact ref
- staged local artifact id
- app-owned capture output promoted into a new layer

Effigy must keep source selection explicit. It must not infer registry refs from
Docker-like strings without `oci://`.

## Stack Manifest Rules

A repo-level stack manifest should declare:

- stack name
- target environment class
- ordered layers
- role for each layer
- source ref/path for each layer when static
- dependency edges when the order alone is not enough
- app hook identity for apply/finalize work when required
- production/dev eligibility
- provenance labels or snapshot identity when known

Minimum first-round stack fields:

- `schema`
- `name`
- `environment`
- `layers`

Minimum first-round layer fields:

- `key`
- `role`
- `source`
- `apply_mode`
- `environment_policy`

Recommended first-round layer fields:

- `depends_on`
- `artifact_kind`
- `snapshot_identity`
- `hook`
- `notes`

The initial schema name is `effigy.state-stack.v1`.

Valid first-round `apply_mode` values:

- `task`
- `artifact`
- `sql`
- `manual`
- `checkpoint`

Valid first-round `environment_policy` values:

- `all`
- `dev-only`
- `non-production`
- `production`
- `capture-only`

The first implementation should parse and validate this shape before it invokes
anything. A planning/reporting surface may land before live apply execution.

The manifest should describe composition, not app semantics. It can say a layer
is `legacy-import` or `uat-capture`; it must not pretend Effigy understands how
to reconcile two exam papers or two media references.

## Apply Rules

Stack apply should resolve into an explicit apply plan and report.

Core rules:

- `structure` runs before imported or captured data layers
- `baseline-seed` runs before imported overlays that depend on static rows
- `dev-overlay` runs only after the baseline is already working
- local and OCI sources resolve through the same artifact substrate before
  app-specific apply hooks run
- apply order must be visible in text and JSON reports

Apply report should include:

- schema id
- environment label
- stack identity
- resolved layers in order
- layer roles
- apply modes
- environment policy decisions
- refs/paths and digests where available
- invoked app hooks or tasks
- result per layer
- lineage id or lineage root
- timestamp

## Capture Rules

Stack capture should package new state into a replayable layer plus lineage.

Core rules:

- capture stages locally first
- OCI publish stays explicit
- capture role must be named
- capture reports must identify the source environment and current lineage root

Likely capture roles:

- `uat-capture`
- `content-overlay`
- `full-capture`

Capture report should include:

- schema id
- source environment label
- parent stack or lineage id
- produced layer role
- artifact ref/path
- digest when available
- invoked app hook or task
- lineage id
- timestamp

## Rebase Rules

Effigy should support the operator workflow for rebase, not the merge logic.

The bounded framework-owned flow is:

1. freeze the working environment
2. capture new-system-authored changes
3. generate a newer imported legacy layer
4. expose both inputs and their lineage clearly
5. rebuild from a clean baseline after app-owned reconciliation

Effigy must not claim to automatically reconcile conflicting domain rows.

## App Hook Boundary

Effigy needs app hooks because replaying layers often means more than piping SQL
into a database.

Framework-owned shape:

- Effigy resolves the layer source
- Effigy stages artifacts and computes lineage
- Effigy invokes the declared app hook with structured context
- the app performs transform/apply/finalize logic
- Effigy records the outer operation report

App hook inputs should include:

- resolved staged paths
- layer role
- artifact kind
- source ref/path
- digest
- environment label
- parent lineage metadata when present

## Media Rules

Media may participate in the same stack, but media semantics stay app-owned.

Effigy may coordinate:

- ordered replay of media-bearing layers
- capture lineage
- staging of binary payloads

Effigy must not own:

- body rewrites
- attachment binding semantics
- per-domain media conflict resolution

## Lineage Rules

Lineage is a first-class outcome of the framework.

The current minimum lineage surface should record:

- stack name or id
- environment label
- ordered layers
- layer roles
- resolved refs/paths
- digests when available
- snapshot identities when declared
- invoked hooks/tasks
- timestamps

The first shipped surface may be command-level JSON/text reports plus staged
metadata. A separate durable persisted ledger can remain future work if the
report contract is explicit now.

First-round lineage relationship:

- artifact operation reports remain the source of truth for individual artifact
  inspect, stage, capture, seed, and dump work
- state-stack reports reference those artifact reports by layer when an
  artifact operation occurs
- the state-stack lineage record is the environment-level rollup across all
  layers
- no durable cross-run ledger is required for the first implementation

Minimum lineage report fields:

- `schema`
- `lineage_id`
- `stack_name`
- `environment`
- `created_at`
- `layers[]`
- `artifact_reports[]`
- `warnings[]`

## Acowtancy Proof Boundary

The first proof case is the UAT freeze/rebuild loop:

1. apply `structure`
2. apply `baseline-seed`
3. apply `legacy-import`
4. optionally apply `dev-overlay`
5. freeze UAT
6. capture `uat-capture`
7. regenerate `legacy-refresh`
8. reconcile offline in app-owned logic
9. rebuild a new baseline

Effigy should prove the orchestration and lineage around that loop.

First implementation slice:

- parse a stack manifest with Acowtancy-shaped layers
- validate role/order/environment policy
- resolve local and `oci://` artifact sources through planning stubs where
  live transport is not needed
- emit a lineage report showing the ordered stack and artifact-operation
  references
- do not execute Farmyard migration hooks yet

Effigy should not absorb:

- MySQL snapshot transform logic
- content-specific conflict rules
- legacy/new media reference reconciliation

## Deferred For Later

- durable persisted cross-run lineage ledger
- direct CLI syntax beyond the first planning surface
- post-go-live sync surfaces
- implementation-specific hook payload schema
- live rebase execution
- app hook execution for Acowtancy/Farmyard

## Next Task

Implement the first state-stack manifest and lineage planning surface without
executing app hooks.
