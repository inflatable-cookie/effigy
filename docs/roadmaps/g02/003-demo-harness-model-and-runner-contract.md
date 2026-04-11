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

### 6.0 Active-Attempt Lifecycle Model

Batch `03.8` decision:

- terminal receipts and latest-attempt summaries are not enough to support
  lifecycle control honestly
- the runner needs a separate active-attempt state layer for in-flight demo
  executions
- the first lifecycle contract allows at most one active attempt per demo
- user-facing lifecycle commands target demo ids first, not raw attempt ids

Why this split is required:

- `demo run` is synchronous today and can already write a terminal receipt
- `demo stop` and later browser status need to know about work that is still in
  flight, not only the last completed receipt
- a receipt is immutable proof evidence; it should not double as a mutable
  process-control record

Minimum active-attempt state:

- stable `attempt_id`
- `demo_id`
- current lifecycle phase (`running`, `stop-requested`, terminalized by the
  runner)
- started-at timestamp
- runnable entrypoint snapshot (`task` or `run`)
- runner-owned handle metadata when available
- latest log/receipt location pointers when known

Lifecycle targeting rules:

- `demo stop <id>` targets a demo id and resolves to that demo's single active
  attempt
- `demo rerun <id>` targets a demo id and always means `start a fresh attempt
  from the current manifest definition`
- attempt ids are still required in runner state and inspection payloads for
  provenance, but they are not part of the first CLI grammar

First-slice lifecycle constraint:

- stop support in the next implementation slice is limited to demos whose
  active process is directly runner-owned and stoppable
- task-backed demos may be runnable and rerunnable without being honestly
  stoppable yet if the generic task/runtime surface does not expose cancellable
  handles
- the contract must surface that distinction instead of pretending every demo
  entrypoint is equally controllable

Rerun rule for the first lifecycle slice:

- `demo rerun <id>` requires that no active attempt currently exists for that
  demo
- if a demo is still running, rerun fails fast and points the operator at
  `demo stop <id>` instead of implicitly chaining stop-and-start behavior

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

### 10.4 Lifecycle Contract Decision

Batch `03.8` delivered:

Lock the active-attempt model before stop/rerun runtime work starts.

Delivered in this planning batch:

- active attempts are now explicitly separate from terminal receipts
- the first lifecycle contract allows one active attempt per demo
- stop and rerun target demo ids in the first CLI grammar
- attempt ids are required in runner state and inspection data, but not yet in
  the top-level command shape
- the next execution slice is constrained to runner-owned stoppable processes
  instead of pretending generic task cancellation already exists

### 10.5 Lifecycle Control Foundation

Batch `03.9` delivered:

Build the first honest lifecycle-control slice on top of the shipped registry,
inspection, and run foundation.

Delivered in this execution slice:

- add runner-owned active-attempt state for demos that are still executing
- add `effigy demo rerun <id>` as a fresh-attempt command on top of the
  existing run surface
- add `effigy demo stop <id>` for demos whose active attempt is directly
  stoppable by the runner
- surface the active-attempt state in `demo inspect`

Still out of scope in this execution slice:

- generic cancellation support for every task-backed demo entrypoint
- multi-attempt concurrency per demo
- TUI/browser implementation
- broad consumer-repo migration work
- chaining stop-and-rerun as one implicit compound action

Why this was the right lifecycle slice:

- it turns lifecycle planning into real product surface without over-claiming
  generic process control that Effigy does not yet own
- it gives the future browser a truthful notion of `running` beyond a terminal
  receipt
- it keeps the batch bounded to one active-attempt model instead of
  reopening CLI targeting later

### 10.6 Next Batch

Batch `03.10` target:

Decide the first bounded post-lifecycle follow-up so the lane does not blur
browser-state polish together with broader runtime-control promises.

In scope for the next planning batch:

- decide whether the next execution slice should prioritize browser-facing
  state polish or broader stoppability/runtime expansion
- define the minimum runner-facing state or query additions the chosen slice
  actually needs
- keep the task-cancellation boundary explicit if generic stoppability still
  depends on deeper runtime handle work

Out of scope for the next planning batch:

- implementing generic task cancellation
- starting TUI/browser implementation
- broad consumer-repo migration work

Follow-on sequence after that decision:

### 10.7 Post-Lifecycle Boundary Decision

Batch `03.10` delivered:

Choose the first bounded post-lifecycle follow-up so the lane does not blur
browser-state polish together with broader runtime-control promises.

Decision:

- prioritize browser-facing state/query polish next
- defer broader stoppability until the runtime can expose cancellable handles
  honestly beyond directly runner-owned demo attempts

Why this is the right next slice:

- the browser/TUI contract already depends on clearer list/inspect/query data
  than the current CLI exposes cleanly
- that work builds directly on the shipped demo registry, inspection, run, and
  lifecycle surfaces
- broader stoppability now turns into a runtime-handle problem, not a small
  demo-surface refinement

### 10.8 Browser State And Query Polish

Batch `03.11` delivered:

Tighten browser-facing state and query polish without starting UI
implementation.

Shipped in the current repo:

- `demo list` now supports focused discovery filters for search, owner, tag,
  mode, cover, status, gap, and stale state
- `demo list` now supports bounded grouping by owner, tag, mode, cover,
  status, or gap
- `demo list` JSON now exposes the applied query, total count, grouped results,
  action availability, and browser-facing freshness/gap state
- `demo inspect` now reports base status, effective status, freshness, and
  explicit action availability alongside active/latest attempt state
- latest-attempt inspection now makes receipt presence and freshness explicit

Why this slice was the right follow-through:

- it gives the later browser/TUI a concrete runner surface to consume before UI
  work starts
- it improves operator-visible proof browsing immediately without promising UI
  behavior in the CLI
- it avoids blurring the demo lane back into generic runtime cancellation work

### 10.9 Self-Hosted Proof Demos

Batch `03.12` delivered:

Pressure-test the shipped demo runner surface against the Effigy repo itself.

Shipped in the current repo:

- `browser-proof-report` now gives the repo a task-backed proof demo that
  generates a small HTML artifact plus concrete list/group/inspect snapshots
- `lifecycle-window` now gives the repo a run-backed proof demo that stays
  active until `demo stop`, making the active-attempt and terminal-receipt
  contract observable without synthetic fixtures
- both demos write normalized receipts and repo-local artifacts under
  `.effigy/demo/`, which means the browser lane can now reason from live proof
  surfaces instead of only abstract contract prose

What this proved:

- the shipped registry, query, inspect, run, stop, and rerun surface is now
  strong enough to support one honest self-hosted proof flow
- browser-proof browsing is already coherent in list/detail form
- the real ergonomic gap is not more CLI query work; it is needing multiple
  terminals to launch, inspect, and stop a live demo attempt

### 10.10 Browser Foundation Slice Decision

Batch `03.13` target:

Decide the first bounded browser/TUI implementation slice on top of the
now-shipped demo query/state surface and the new self-hosted proof demos.

In scope for the next planning slice:

- define the first honest browser/TUI implementation boundary around sidebar or
  list browsing, detail inspection, and in-browser action dispatch
- decide what the interactive client should consume directly from shipped demo
  runner outputs
- keep the batch constrained to browser foundation rather than runtime
  expansion or terminal emulation

Out of scope for the next planning slice:

- implementing the TUI/browser itself beyond the bounded foundation choice
- broadening generic task/runtime cancellation
- multi-attempt history or queueing
- consumer-repo migration work

Follow-on sequence after that slice:

- first browser/TUI implementation batch focused on list/detail/actions
- broader stoppability once the generic task/runtime surface can expose
  cancellable handles honestly
- broad consumer-repo migration work

### 10.11 Browser List/Detail Foundation

Batch `03.14` delivered:

Ship the first honest interactive browser client on top of the already-shipped
demo runner surface.

Shipped in the current repo:

- `effigy demo browser` now opens a bounded TUI browser for repo-owned demos
- the browser renders grouped list browsing on the left and detail inspection
  for the selected demo on the right
- the browser delegates `run`, `stop`, and `rerun` through the shipped demo
  runner instead of inventing a second execution model
- the browser is already proven against the repo's self-hosted
  `browser-proof-report` and `lifecycle-window` demos

What this proved:

- the shipped query/state surface was strong enough to support a real browser
  without reopening the registry or lifecycle contracts
- the biggest remaining browser question is no longer list/detail structure; it
  is which follow-up affordance matters more next: live log visibility or
  artifact-opening
- broader runtime cancellation is still a separate runtime problem and should
  not be smuggled back in through browser follow-up work

### 10.12 Browser Follow-Up Slice Decision

Batch `03.15` delivered:

Choose the next bounded browser follow-up slice now that the foundation browser
exists.

Decision:

- prioritize artifact-opening affordances next
- defer live log visibility until after artifact inspection is usable from
  inside the browser

Why this is the right next slice:

- `browser-proof-report` already proves the value of opening a real artifact
  from the browser, because its HTML report is the most operator-meaningful
  proof output in the current self-hosted set
- `lifecycle-window` also benefits from direct access to `status.txt`,
  `heartbeat.txt`, and `events.log` without forcing the next batch into log
  streaming semantics
- live log visibility pulls the lane toward tailing, stream refresh, and
  terminal-shape questions that are materially broader than the current
  browser-follow-up need

### 10.13 Browser Artifact-Affordance Slice

Batch `03.16` delivered:

Implement bounded artifact-opening affordances in the demo browser.

Shipped in the current repo:

- artifact selection inside `demo browser` with `[` and `]`
- one bounded open action with `o` for the selected artifact path
- honest failure messaging when the artifact is missing or no opener can be
  launched
- no widening into live log streaming or terminal emulation

What this proved:

- the browser can now act on runner-owned proof artifacts instead of only
  listing them as inert strings
- artifact access was the tighter post-foundation follow-up than log streaming
- the next browser question is now whether live log visibility is still the
  right next bounded slice after artifact access is usable

### 10.14 Post-Artifact Follow-Up Boundary

Batch `03.17` delivered:

Choose live log visibility as the next honest browser slice after
artifact-opening.

What this settled:

- artifact access was the right tighter follow-up immediately after the first
  browser foundation
- with artifact opening shipped, the next real operator gap is current output
  visibility for the selected demo
- no tighter browser-only affordance remains ahead of logs

Still deferred:

- terminal emulation
- broader generic runtime cancellation
- multi-attempt history or queueing
- desktop-client decisions

### 10.15 Browser Live-Log Visibility Slice

Batch `03.18` delivered:

Implement bounded live log visibility inside `effigy demo browser`.

What shipped:

- recent stdout/stderr visibility in the detail pane for the selected demo
- active-attempt output when runner-owned logs are available
- latest terminal output when available for completed attempts
- honest missing-log handling instead of fake empty panes

What this proved:

- the browser can consume runner-owned log paths without widening into terminal
  emulation
- recent-output visibility is the right next operator affordance after artifact
  access
- the next browser question should now be decided explicitly instead of assumed

Still deferred:

- terminal emulation
- arbitrary stdin interaction
- broader generic runtime cancellation
- multi-attempt history or queueing

### 10.16 Post-Live-Log Follow-Up Boundary

Batch `03.19` delivered:

Choose the next bounded browser follow-up after live log visibility.

What this settled:

- the next honest browser gap is in-browser registry narrowing, not richer log
  handling
- the browser should consume the already-shipped `demo list` query contract
  instead of inventing browser-only filtering semantics
- richer log handling and artifact/detail polish remain possible later, but
  they are not the next bounded slice exposed by the self-hosted demos

Why query controls next:

- the browser already exposes grouping, lifecycle actions, artifact access, and
  recent output for a selected demo
- the current TUI still cannot narrow the registry by search, owner, status,
  gap, or stale state without leaving the browser
- the shipped `demo list` surface already defines those query semantics, so the
  browser can adopt them without widening the runner contract

Still deferred:

- richer live-log handling beyond bounded recent output
- artifact preview or richer detail rendering
- terminal emulation
- broader generic runtime cancellation
- multi-attempt history or queueing
- desktop-client decisions

### 10.17 Browser Query Controls Slice

Batch `03.20` target:

Implement bounded browser query controls on top of the shipped `demo list`
contract.

In scope for the next implementation slice:

- in-browser query state for the highest-signal existing filters
- visible operator feedback about active query constraints
- honest empty-state handling when filters narrow the registry to no results
- reuse of existing runner query semantics rather than browser-only logic

Out of scope for the next implementation slice:

- new query semantics not already shipped through `demo list`
- richer log streaming or terminal emulation
- artifact preview or richer rendering
- multi-attempt history or queueing

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

Use the active `g02.003` strict lane to implement bounded browser query
controls without reopening broader runtime-cancellation scope.
