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

Minimum likely fields:

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

### 5.2 Task-backed vs distinct

The likely answer is task-adjacent:

- demos stay distinct verification objects
- demos may reference tasks or reuse execution primitives
- demos get richer semantics than generic tasks

### 5.3 Inline vs split config

The model should work inline in `effigy.toml` immediately. It should also be
designed so future manifest composition can split demos cleanly without demo-only
loader semantics.

## 6) Runner Contract

The runner must own:

- discovery and listing
- run / stop / rerun
- timeout and cancellation behavior
- status transitions
- log collection
- artifact/receipt normalization
- coverage and gap reporting
- machine-readable output for later clients

Minimal status model to evaluate:

- `planned`
- `ready`
- `running`
- `passed`
- `failed`
- `broken`
- `missing`

## 7) TUI Browser Contract

The first client should likely be a TUI browser that can:

- list demos in a sidebar/browser
- group and filter them
- show status badges
- show gaps and missing proof
- run / stop / rerun demos
- inspect logs, receipts, and artifacts
- give clear operator feedback without command hunting

The TUI should not own project-specific semantics beyond what the runner model
already exposes.

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

- [ ] Define demo identity, metadata, proof intent, and ownership fields
- [ ] Define runnable entrypoint and dependency model
- [ ] Define artifact/receipt expectations

### Batch 03.2 - Runner Semantics

- [ ] Define discovery, execution, stop/cancel, timeout, and rerun semantics
- [ ] Define minimal lifecycle/state model
- [ ] Define failure and broken-demo behavior

### Batch 03.3 - Coverage and Gap Model

- [ ] Define how repos express covered vs planned vs missing proof
- [ ] Define operator-visible gap reporting
- [ ] Define how stale or broken proof is surfaced

### Batch 03.4 - TUI Contract

- [ ] Define the browser/list contract for a first TUI client
- [ ] Define logs/artifacts/receipts drilldown requirements
- [ ] Define the minimum runner data the TUI needs

### Batch 03.5 - Pilot Readiness

- [ ] Reconcile the contract against Signal's existing demo surface
- [ ] Decide what can migrate directly vs what is script-harness debt
- [ ] Leave a bounded implementation lane only after the model is coherent

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

Use the active `g02.003` strict lane to decide the demo object model and
registry boundary first, then move to runner semantics before any TUI or
desktop client work begins.
