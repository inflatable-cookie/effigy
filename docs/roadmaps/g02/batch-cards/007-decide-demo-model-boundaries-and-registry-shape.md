# 007 Decide Demo Model Boundaries And Registry Shape

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Lock the first bounded demo-harness model decision:

- what a demo object is
- how it differs from a task
- where the registry lives
- what minimum metadata must be first-class

## In Scope

- define the minimum demo declaration shape
- define task-backed vs task-adjacent vs fully distinct boundaries
- define repo-owned registry posture in Effigy config
- define the minimum inline-vs-composed config expectation without inventing
  demo-local file loading

## Out Of Scope

- runner lifecycle implementation
- TUI implementation
- desktop-client decisions
- project-specific migration plans

## Acceptance Criteria

- `g02.003` clearly states what makes a demo a first-class object
- the registry boundary is explicit and Effigy-owned
- task reuse is allowed without collapsing the model into generic tasks
- the next batch can move onto runner semantics instead of relitigating object
  identity

## Outcome

Closed with these decisions:

- demo registry root: `[demos]`
- demo identity shape: `[demos.<id>]` with stable map-key ids instead of an
  anonymous array
- demo boundary: task-adjacent but semantically distinct from normal tasks
- registry ownership: repo-owned Effigy manifest data, not a second external
  registry
- config posture: inline-first and future composition-compatible, with no
  demo-local include semantics

## Next Task

Move to runner semantics next: discovery, run/stop lifecycle, artifact/receipt
boundaries, and the minimum status model the TUI/browser layer will depend on.

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch drifts into runner lifecycle or TUI detail before the object model
  is fixed
- config-shape discussion depends on new composition features beyond what is
  already shipped

## Next Task

Complete this planning batch, then leave the next move explicit as either
runner/lifecycle semantics or coverage/gap modeling, depending on which
remaining ambiguity is more blocking.
