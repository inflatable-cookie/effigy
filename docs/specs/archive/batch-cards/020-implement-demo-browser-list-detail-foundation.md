# 020 Implement Demo Browser List/Detail Foundation

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Ship the first honest demo browser/TUI foundation on top of the now-shipped
registry, query, inspect, run, and lifecycle surface.

## In Scope

- add a browser-oriented TUI entrypoint for demos
- provide a left-side list or grouped browser view driven by the shipped demo
  registry/query state
- provide a detail view for the selected demo showing proof intent, coverage,
  tags, action availability, active attempt state, latest receipt state, and
  artifact paths
- support refresh plus bounded `run`, `stop`, and `rerun` actions from inside
  the browser
- keep action execution honest by delegating through the shipped demo runner
  rather than inventing a second execution model

## Out Of Scope

- generic task/runtime cancellation beyond current demo-owned stoppability
- full terminal streaming or embedded terminal emulation
- rich artifact rendering beyond listing artifact paths and receipt summaries
- multi-attempt history, queueing, or project-specific demo UI

## Acceptance Criteria

- Effigy has a first real interactive demo browser entrypoint
- operators can browse demos without hunting through CLI commands manually
- operators can inspect one selected demo and trigger `run`, `stop`, or `rerun`
  from the same surface
- the implementation stays grounded in the shipped runner data model rather than
  reopening contract questions

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch drifts into generic runtime cancellation design
- the batch starts rendering rich project-specific artifacts instead of browser
  foundation state
- the batch reopens settled demo model or lifecycle contracts

## Next Task

Decide whether the next browser slice should prioritize live log visibility or
artifact-opening affordances now that the list/detail foundation is shipped.
