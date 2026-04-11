# 003 - Demo Harness Model and Runner Contract

Generation: `g02`

Status: In Progress
Owner: Platform
Created: 2026-04-11
Depends on: 002, 027, 028

## Vision Alignment

Effigy already owns repo-local execution, validation posture, task discovery,
and operator-facing command surfaces. What it does not yet own is a first-class
verification/demo model for proving small slices of real product behavior.

Across projects, demo proof currently tends to degrade into ad hoc scripts,
one-off receipts, and project-specific orchestration layers. Tests remain
useful, but they do not answer a different operator question:

- what product proof exists here
- how do I run it
- what did it verify
- what artifacts or receipts did it produce
- what is still missing or broken

This roadmap defines the model and runner contract for first-class demo proof in
Effigy.

## Primary Tags

- `OPERATE`
- `CONTRACT`
- `MAINT`
- `ROUTE`

## Target Envelope

- Effigy has a first-class demo surface such as `effigy demo`.
- Demo declarations are repo-owned verification objects, not just generic task
  aliases.
- Demo discovery, execution, receipts, artifacts, and coverage/gap reporting
  use one coherent model across projects and languages.
- The first interactive client can be a TUI browser on top of that model.
- A later desktop client, if it exists, consumes the same runner contract
  instead of inventing its own registry or execution semantics.

## Vision Target Delta

- Move from `project-local demo scripts and receipts with duplicated
  orchestration logic` toward `one Effigy-native demo verification model with a
  reusable runner and operator-visible browser semantics`.

## 1) Problem

Signal proved that manifests, scenarios, receipts, and rendered proof artifacts
are viable. It also exposed the weak point:

- orchestration and rendering logic becomes script-heavy
- proof discovery is inconsistent
- coverage and gap visibility are implicit
- operator ergonomics decay as demos multiply

If Effigy does not own this surface, every project will slowly grow its own
verification harness dialect.

## 2) Goals

- [ ] Define a first-class demo declaration model
- [ ] Define how demos differ from normal tasks
- [ ] Define runner semantics for discovery, run/stop/status/artifacts
- [ ] Define coverage and gap reporting as first-class concepts
- [ ] Define the browser contract needed by a first TUI client
- [ ] Keep the model compatible with both inline manifest config and future
      composed config
- [ ] Keep the desktop-client question explicitly deferred

## 3) Non-Goals

- [ ] No desktop app decision in this lane
- [ ] No project-specific UI logic embedded in the model
- [ ] No “just treat demos as random tasks with nicer names”
- [ ] No standalone second registry outside Effigy
- [ ] No early commitment to artifact rendering formats beyond minimum receipts

## 4) Design Rules

### 4.1 Model first, UI second

The runner contract comes before the TUI. The TUI should consume discovery,
state, logs, artifacts, and gaps from the runner rather than defining them.

### 4.2 Demos are semantically distinct from tasks

Tasks are generic execution selectors. Demos are verification objects with
identity, proof intent, operator meaning, and inspectable outputs.

A demo may reuse tasks internally, but it must not collapse into “a task with a
friendlier label”.

### 4.3 One registry, repo-owned

Demo ownership should live in Effigy repo config, not a second external system.
Composition may later split config files, but the contract still belongs to the
manifest model.

### 4.4 Coverage is explicit

The system should not only know what demos exist. It should also surface what is
planned, missing, broken, or stale.

## 5) First-Class Model Questions

### 5.1 Demo declaration shape

Batch `03.1` decision:

- demo registry root: `[demos]`
- each demo is declared as `[demos.<id>]`
- ids are stable map keys, not anonymous array entries
- demos are repo-owned manifest objects, not external registry items

Minimum first-class fields:

- id
- title/summary
- proof intent
- owner/scope
- mode (`headless`, `interactive`, `hybrid`)
- runnable entrypoint
- dependencies/prerequisites
- expected artifacts/receipts
- status posture (`planned`, `ready`, `broken`, `missing`, etc.)
- tags/grouping fields for browser navigation

Illustrative direction:

```toml
[demos.login-smoke]
title = "Login Smoke"
summary = "Proves the local login flow reaches a successful authenticated state."
owner = "auth"
proof = "Verify that the default local login journey works end to end."
mode = "interactive"
status = "ready"
tags = ["auth", "smoke"]
artifacts = ["receipts/login-smoke.json", "artifacts/login-smoke.png"]
task = "demo:login"
```

Notes:

- `id` remains the stable demo identity, but it is carried by the map key
  (`login-smoke`) rather than duplicated as a required inner field.
- `task = "..."` is illustrative of reuse, not a decision that demos collapse
  into generic task routing.
- inline config must work immediately; future split-file config uses
  `[manifest].include`, not a demo-only loader.
- `covers = ["..."]` is part of the first-class metadata because proof
  discovery and gap reporting need explicit coverage claims rather than
  inference from titles or tags alone.

### 5.2 Task-backed vs distinct

Batch `03.1` decision:

- demos stay distinct verification objects
- demos may reference tasks or reuse execution primitives
- demos get richer semantics than generic tasks
- demo discovery and later execution should live under a dedicated Effigy demo
  surface, not generic `effigy <task>` routing

Practical boundary:

- tasks remain generic execution units
- demos wrap proof intent, status, artifacts, and browser identity around one
  runnable entrypoint
- a demo may point at a task, command, scenario, or later richer runner-owned
  execution shape, but the registry and semantics stay demo-specific

### 5.3 Inline vs split config

Batch `03.1` decision:

- inline demo config in `effigy.toml` is mandatory for the first contract
- future split-file config must use the shipped general composition model from
  `g02.002`
- this lane must not invent demo-local import/include semantics

That keeps the registry Effigy-owned while still letting larger repos split
config later through ordinary manifest composition.

## 6) Runner Contract

Batch `03.2` decision:

- runner surface: `effigy demo`
- the runner owns discovery, selection, execution, stop/rerun, receipt writing,
  and machine-readable state
- the runner does not own rich artifact rendering, browser layout, or
  project-specific interaction logic

The runner must own:

- discovery and listing
- run / stop / rerun
- timeout and cancellation behavior
- status transitions
- log collection
- artifact/receipt normalization
- coverage and gap reporting
- machine-readable output for later clients

Minimum lifecycle/status model:

- `planned`
- `ready`
- `running`
- `passed`
- `failed`
- `broken`
- `missing`

Semantics:

- `planned`
  - proof is intended but not yet runnable
- `ready`
  - runnable and expected to work, but not currently executing
- `running`
  - active execution is in progress
- `passed`
  - most recent execution produced a valid receipt and satisfied the proof goal
- `failed`
  - execution completed but the proof goal was not satisfied
- `broken`
  - the demo definition or prerequisite/runtime posture is invalid enough that
    execution cannot honestly proceed
- `missing`
  - the proof area is known, but no runnable demo object exists yet

Operator actions:

- `list`
- `inspect`
- `run`
- `stop`
- `rerun`

Lifecycle rules:

- `run` starts a new execution from `ready`, `passed`, `failed`, or `broken`
  once prerequisites allow it
- `stop` applies only to `running` demos and should transition through a
  terminated execution outcome rather than pretending the attempt never existed
- `rerun` is an explicit fresh run with a new receipt, not a mutable reset of
  the previous attempt

Execution boundary:

- each demo resolves to one runnable entrypoint
- that entrypoint may be task-backed, command-backed, or later scenario-backed
- the runner may reuse task execution primitives internally, but demo execution
  remains a demo-owned surface rather than generic task dispatch

Receipt and artifact boundary:

- receipts are runner-normalized verification records for each attempt
- artifacts are repo-produced outputs associated with the attempt
- the runner tracks artifact references and receipt metadata; it does not own
  rich artifact rendering formats in this lane

## 6.1 Coverage And Gap Model

Batch `03.3` decision:

- all known proof obligations live explicitly in the `[demos.<id>]` registry
- the browser must not infer missing proof from silence alone
- demos carry explicit coverage claims through `covers = ["area.key"]`
- `stale` is a freshness overlay, not a new lifecycle status

Coverage interpretation by base status:

- `planned`
  - known proof obligation, not runnable yet
- `missing`
  - known proof obligation, no runnable proof surface exists yet
- `broken`
  - runnable proof surface exists in principle, but it is not currently
    trustworthy or executable enough to count as healthy proof
- `ready`, `running`, `passed`, `failed`
  - proof surface exists

Freshness overlay:

- `stale` marks an existing proof surface whose latest receipt can no longer be
  trusted as current enough
- stale does not replace the base lifecycle state; it decorates it
- the exact freshness algorithm can remain implementation work, but the browser
  must be able to surface stale proof separately from broken or missing proof

Minimum browser-facing gap classes:

- existing proof
- planned proof
- missing proof
- broken proof
- stale proof

Minimum metadata needed for gap visibility:

- `covers`
- `status`
- `title`
- `summary`
- `owner`
- latest receipt/freshness info when available

That is enough for the browser to answer:

- what proof exists
- what proof is planned
- what proof is missing
- what proof is broken
- what proof is stale

## 7) TUI Browser Contract

Batch `03.4` decision:

- the first interactive client is a TUI browser, not a bespoke per-project app
- its primary navigation surface is a sidebar/list of demo records driven by the
  explicit `[demos.<id>]` registry
- the browser consumes runner and coverage data directly; it must not infer core
  identity, status, or gap meaning from naming conventions or file layout

Primary browser responsibilities:

- show the full known demo registry
- make gap classes visible without requiring a run command first
- support fast operator navigation between demos
- expose the minimum run/stop/rerun actions without command hunting
- let operators inspect the latest proof evidence attached to a demo

Minimum list/sidebar model:

- one row per demo id
- stable ordering with support for grouped views
- enough compact metadata to answer:
  - what this demo is
  - whether proof exists
  - whether it is healthy now
  - whether it is stale, broken, planned, or missing

Minimum row fields:

- `id`
- `title`
- base lifecycle status
- stale overlay when present
- gap class
- owner
- tags or category hints when present

Minimum grouping dimensions:

- owner
- tag
- mode
- coverage area via `covers`
- lifecycle/gap class

Minimum filtering dimensions:

- text search on id/title/summary
- owner
- tag
- mode
- coverage area
- status
- gap class
- stale only

Badge model:

- one primary badge for base lifecycle status
- one secondary gap/freshness indicator when needed
- `stale` remains an overlay rather than replacing the base lifecycle state
- `missing` and `planned` must be visually distinguishable from `broken`

Minimum drilldown/inspect surface for the selected demo:

- `title`
- `summary`
- proof intent
- owner
- `covers`
- tags
- mode
- runnable entrypoint reference
- prerequisites/dependencies
- latest known receipt summary when available
- latest execution outcome/state
- artifact references
- recent logs or log handle

Minimum receipt/artifact drilldown expectations:

- show whether a receipt exists for the latest attempt
- show where the latest receipt came from
- show the receipt outcome summary
- list artifact references attached to that attempt
- do not require the TUI to render rich artifact formats in this lane

Minimum runner data the TUI depends on:

- full demo registry with declared metadata
- current base lifecycle status
- stale overlay/freshness metadata when present
- latest receipt summary and source
- artifact references
- runnable entrypoint/action availability
- execution progress/state for running demos
- coverage claims and derived gap class

The TUI should not own project-specific semantics beyond what the runner model
already exposes, and this lane does not decide widget layout, palette, pane
geometry, or desktop-client behavior.

## 8) Deferred Client Questions

Do not decide yet:

- desktop runtime/framework
- GPUI vs Electron vs anything else
- richer long-form visualization beyond what the runner contract requires
- whether some projects eventually want project-specific polished proof apps

Those are client-layer questions, not model-layer questions.

## 9) Relationship To Manifest Composition

This lane should not invent its own external file loading model.

- inline demo config must be sufficient for early proof
- future split-file config should use the general composition contract from
  `g02.002`
- the demo roadmap must not depend on composition shipping first to define the
  model

## 10) Execution Plan

### Batch 03.1 - Model Contract

- [x] Define demo identity, metadata, proof intent, and ownership fields
- [x] Define runnable entrypoint and dependency model boundary at the model
      level
- [x] Define the registry shape as repo-owned `[demos.<id>]` data

### Batch 03.2 - Runner Semantics

- [x] Define discovery, execution, stop/cancel, timeout, and rerun semantics
- [x] Define minimal lifecycle/state model
- [x] Define failure and broken-demo behavior

### Batch 03.3 - Coverage and Gap Model

- [x] Define how repos express covered vs planned vs missing proof
- [x] Define operator-visible gap reporting
- [x] Define how stale or broken proof is surfaced

### Batch 03.4 - TUI Contract

- [x] Define the browser/list contract for a first TUI client
- [x] Define logs/artifacts/receipts drilldown requirements
- [x] Define the minimum runner data the TUI needs

### Batch 03.5 - Pilot Readiness

- [x] Reconcile the contract against Signal's existing demo surface
- [x] Decide what can migrate directly vs what is script-harness debt
- [x] Leave a bounded implementation lane only after the model is coherent

### 10.1 Signal Pilot Reconciliation

Batch `03.5` decision:

Signal's existing `demos/` surface validates the core Effigy direction, but it
also makes the implementation boundary obvious.

What maps directly into Effigy's first-class demo model:

- manifest-backed demo identity with stable ids and explicit titles/summaries
- repo-owned runnable entrypoints instead of operator-memory commands
- explicit scenario/operator-notes references for human checks that matter
- machine-readable receipts attached to each proof attempt
- artifact companions such as rendered HTML views referenced from the proof
  record
- explicit proof coverage claims, even though Signal currently expresses some
  of them through `covered_crates` and the standalone coverage matrix
- repo-owned status posture instead of implicit “this probably works”

What should become runner-owned normalization rather than staying
project-specific:

- status vocabulary translation from Signal's local `active`/`planned`/
  `deferred` posture into Effigy's shared lifecycle/gap model
- receipt normalization so the runner emits one Effigy demo receipt contract
  even when projects currently have local receipt families
- artifact attachment and latest-attempt summary as runner state instead of ad
  hoc pairing of `*.receipt.json` and `*.view.html`
- coverage/gap reporting as a registry-driven runner view rather than a
  separate handwritten matrix as the source of truth
- inspection output that joins manifest metadata, latest receipt outcome,
  artifact references, and runnable entrypoint posture in one browser-friendly
  shape

What Signal exposes as current harness debt rather than model truth:

- one flat per-demo script runner for nearly every official demo
- duplicated launch/render/receipt plumbing across Python files
- project-local temporary web serving and HTML generation inside the runner
  layer
- a separate coverage-matrix maintenance surface that can drift from manifests
- script-owned orchestration decisions that should eventually live in Effigy
  runner semantics

Implementation boundary implied by the pilot:

- the first Effigy implementation slice should own registry loading, demo
  listing/inspection, and a normalized receipt/artifact state model
- runner execution can start with task-backed and command-backed launch support
  instead of solving every Signal-specific script concern immediately
- the pilot does not justify building a desktop client or bespoke visual shell
  first
- Signal migration should follow the runner foundation rather than defining it

### Batch 03.6 - Implementation Slice

- [x] Decide the first bounded runner foundation slice
- [x] Keep execution actions and browser work out of that first slice
- [x] Leave a concrete execution card for registry loading and inspection

### 10.2 First Implementation Slice

Batch `03.6` decision:

The first implementation slice stays deliberately foundation-only. It should
prove that Effigy can own the demo registry and inspection surface before it
owns execution orchestration or browser interaction.

In scope for the first execution slice:

- manifest-backed demo registry loading from `[demos.<id>]`
- schema and doctor support for the demo registry
- `effigy demo list` as a text and JSON discovery surface
- `effigy demo inspect <id>` as a text and JSON inspection surface
- normalized latest-attempt state when a receipt or artifact reference already
  exists
- path and provenance reporting for the selected demo record

Out of scope for the first execution slice:

- `effigy demo run`
- `effigy demo stop`
- `effigy demo rerun`
- TUI/browser implementation
- consumer-repo migration work

Minimum normalized latest-attempt shape for the first slice:

- latest outcome/status
- receipt source/path
- artifact references
- latest-attempt summary text when available
- enough state to distinguish `no recorded attempt yet` from `recorded attempt
  failed` or `recorded attempt passed`

Why this is the right first slice:

- it exercises the new registry as real product surface
- it proves the inspection contract the future TUI depends on
- it forces receipt/artifact normalization before execution work hides the data
  model problem
- it avoids pulling Signal's script orchestration debt into the first batch

Follow-on sequence after the first slice:

- runner execution foundation (`run` and normalized attempt creation)
- then stop/rerun semantics
- then the first TUI/browser client on top of the now-real discovery and
  inspection surface

Implementation status:

- shipped in the current repo through manifest-backed `[demos.<id>]` loading
- `effigy demo list` now provides text and JSON discovery
- `effigy demo inspect <id>` now provides text and JSON inspection with source
  provenance plus normalized latest-attempt state
- `effigy demo run <id>` now provides text and JSON execution for task-backed
  and run-backed demos
- normalized latest-attempt receipts are now written during execution, using a
  default `.effigy/demo/receipts/<demo-id>.json` path when the manifest does
  not declare `receipt`
- schema, doctor, and config-reference surfaces now understand the demo
  registry contract

### 10.3 Run And Attempt Foundation

Batch `03.7` delivered:

Build runner execution on top of the shipped registry and inspection
foundation.

Delivered in this execution slice:

- `effigy demo run <id>` as a text and JSON execution surface
- support for both task-backed and run-backed demo entrypoints
- normalized attempt creation so `demo inspect` reflects newly executed proof
- baseline pass/fail outcome reporting and receipt writing

Still out of scope in this execution slice:

- `effigy demo stop`
- `effigy demo rerun`
- TUI/browser implementation
- broad consumer-repo migration work

Why this was the right second slice:

- it turned the registry foundation into a real proof-execution surface
- it forced Effigy to own receipt writing instead of only parsing pre-existing
  repo artifacts
- it kept the lifecycle boundary honest by stopping short of process-control
  behavior that still needs an explicit active-attempt model

### 10.4 Next Batch

Batch `03.8` target:

Lock the lifecycle control model before stop/rerun runtime work starts.

In scope for the next execution slice:

- define what counts as an active demo attempt
- decide the target shape for `demo stop` and `demo rerun`
- decide the minimum persisted state/process handle model those commands need
- leave one bounded implementation card for lifecycle control if the model is
  coherent

Out of scope for the next execution slice:

- implementing `effigy demo stop`
- implementing `effigy demo rerun`
- TUI/browser implementation
- broad consumer-repo migration work

## 11) Acceptance Criteria

- [ ] Effigy has a clear first-class demo model that is not reducible to random
      tasks
- [ ] The runner contract is clear enough that a TUI can be built on top of it
- [ ] Coverage and gap visibility are first-class, not implicit
- [ ] The lane explicitly defers desktop-client decisions

## 12) Risks and Mitigations

- [ ] Risk: demos stay too task-shaped and the browser becomes shallow
  - Mitigation: keep proof intent, status, and artifact semantics distinct
- [ ] Risk: the model gets bloated before a first proof lane exists
  - Mitigation: start with the minimum state and artifact model needed for
    operator proof
- [ ] Risk: feature-local config loading sneaks in before manifest composition
  - Mitigation: require inline viability first and reserve split config for the
    general composition contract

## Next Task

Use the active `g02.003` strict lane to decide active-attempt, stop, and rerun
semantics next, then leave one bounded lifecycle-control card behind it.
